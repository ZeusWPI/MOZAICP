use super::*;
use crate::generic::{
    BrokerHandle, Handler, LinkOperation, LinkSpawner, Operation, ReactorID, Receiver, Sender,
};
use crate::graph;

use tracing::{instrument, Span};

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use futures::stream::Stream;
use futures::task::{Context, Poll};
use futures::Future;
use std::pin::Pin;

/// Macro to create reactor handle
/// This does not borrow the entire Reactor like a function would
macro_rules! reactorHandle {
    ($e:expr) => {
        ReactorHandle::new(&$e.channels.0, &$e.id, &mut $e.inner_ops, &mut $e.broker);
    };
}

/// Gives the option for an init function on a reactor
pub trait ReactorState<K, M> {
    const NAME: &'static str;
    fn init<'a>(&mut self, _: &mut ReactorHandle<'a, K, M>) {}
}

/// Blanket implementation for ()
impl<K, M> ReactorState<K, M> for () {
    const NAME: &'static str = "Empty";
}

///
/// Reactor is the meat and the potatoes of MOZAIC
/// You can register Handers (just functions)
/// That are called when the correct data T is being handled
///
pub struct Reactor<S, K, M>
where
    K: Hash + Eq,
    S: ReactorState<K, M>,
{
    id: ReactorID,

    broker: BrokerHandle<K, M>,
    state: S,
    msg_handlers:
        HashMap<K, Box<dyn for<'a> Handler<S, ReactorHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,

    links: HashMap<
        ReactorID,
        (
            Box<
                dyn for<'a, 'b> Handler<
                        (),
                        ReactorHandle<'b, K, M>,
                        &'a mut LinkOperation<'a, K, M>,
                    > + Send,
            >,
            Span,
            bool,
        ),
    >,

    channels: (Sender<K, M>, Receiver<K, M>),

    inner_ops: VecDeque<InnerOp<K, M>>,
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
    S: ReactorState<K, M>,
{
    /// Pretty ugly function to create a reactor
    /// TODO: Make less of an eye sore
    pub fn new(
        id: ReactorID,
        broker: BrokerHandle<K, M>,
        params: CoreParams<S, K, M>,
        channels: (Sender<K, M>, Receiver<K, M>),
    ) -> Self {
        let (state, msg_handlers) = params.consume();
        Reactor {
            id,
            broker,
            state,
            msg_handlers,
            links: HashMap::new(),
            channels,
            inner_ops: VecDeque::new(),
        }
    }

    /// Returns a handle to the reactor
    pub fn get_handle<'a>(&'a mut self) -> ReactorHandle<'a, K, M> {
        reactorHandle!(self)
    }

    /// Handles an internal message
    ///
    /// First looking up his own internal message handler for that message
    /// Then letting all links handle that message
    #[instrument(skip(self, msg, id))]
    fn handle_internal_msg(&mut self, id: K, mut msg: M, target: TargetReactor) {
        let mut handle = reactorHandle!(self);

        match target {
            TargetReactor::All => {
                let mut found = false;
                let mut state = ();

                for (_, (handler, span, _)) in self.links.iter_mut() {
                    let _enter = span.enter();
                    handler.handle(
                        &mut state,
                        &mut handle,
                        &mut LinkOperation::InternalMessage(&id, &mut msg),
                    );
                    found = true;
                }
                if let Some(h) = self.msg_handlers.get_mut(&id) {
                    found = true;
                    h.handle(&mut self.state, &mut handle, (&id, &mut msg));
                }

                if !found {
                    trace!("No handler found!");
                }
            }
            TargetReactor::Links => {
                let mut found = false;
                let mut state = ();

                for (_, (handler, span, _)) in self.links.iter_mut() {
                    let _enter = span.enter();
                    handler.handle(
                        &mut state,
                        &mut handle,
                        &mut LinkOperation::InternalMessage(&id, &mut msg),
                    );
                    found = true;
                }

                if !found {
                    trace!("No handler found!");
                }
            }
            TargetReactor::Reactor => {
                if let Some(h) = self.msg_handlers.get_mut(&id) {
                    h.handle(&mut self.state, &mut handle, (&id, &mut msg));
                } else {
                    trace!("No handler found!");
                }
            }
            TargetReactor::Link(target) => {
                if let Some((handler, span, _)) = self.links.get_mut(&target) {
                    let _enter = span.enter();
                    trace!("Sending to {:?}", target);
                    handler.handle(
                        &mut (),
                        &mut handle,
                        &mut LinkOperation::InternalMessage(&id, &mut msg),
                    );
                } else {
                    trace!("No handler found!");
                }
            }
        }
    }

    /// Handles an external message
    ///
    /// This message is sent by a link to this reactor
    /// Look up the corresponding link and letting him/her handle the message
    #[instrument(skip(self, id, msg))]
    fn handle_external_msg(&mut self, origin: ReactorID, id: K, mut msg: M) {
        let mut handle = reactorHandle!(self);

        let mut m = LinkOperation::ExternalMessage(&id, &mut msg);

        if self
            .links
            .get_mut(&origin)
            .map(|(handler, span, _)| {
                let _enter = span.enter();
                handler.handle(&mut (), &mut handle, &mut m)
            })
            .is_none()
        {
            error!("No link found to reactor {}", origin);
        }
    }

    /// Opens a link to the target reactor
    /// You can only have a most one link to a reactor
    #[instrument(skip(self, spawner, cascade))]
    fn open_link(&mut self, target: ReactorID, spawner: LinkSpawner<K, M>, cascade: bool) {
        graph::add_edge(&self.id, &target);
        trace!(%target, source = %self.id, "Open link");

        let tx = self.broker.get(&target);
        let handles = (self.channels.0.clone(), tx, self.id, target);
        self.links.insert(
            target,
            (
                spawner(handles),
                trace_span!("Link", %target, source = %self.id),
                cascade,
            ),
        );
    }

    /// Closes a link to the target reactor
    #[instrument(skip(self))]
    fn close_link(&mut self, target: ReactorID) {
        graph::remove_edge(&self.id, &target);

        let mut handle = reactorHandle!(self);

        if let Some((mut link, span, cascade)) = self.links.remove(&target) {
            let _enter = span.enter();
            trace!(%target, source = %self.id, "Close link");

            link.handle(&mut (), &mut handle, &mut LinkOperation::Close());
            if cascade {
                self.inner_ops.push_back(InnerOp::Close());
            }
        } else {
            info!(%target, source = %self.id, "Failed to close link");
        }
    }

    #[instrument(skip(self))]
    fn close(&mut self) {
        let mut handle = reactorHandle!(self);
        let mut state = ();

        for (_, (handler, span, _)) in self.links.iter_mut() {
            let _enter = span.enter();
            handler.handle(&mut state, &mut handle, &mut LinkOperation::Close());
        }

        // Stop Future
        self.channels.1.close();
    }
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
    S: ReactorState<K, M>,
{
    /// Initializes the spawned reactor
    pub fn init(&mut self) {
        let mut handle = reactorHandle!(self);

        self.state.init(&mut handle);

        while let Some(op) = self.inner_ops.pop_back() {
            match op {
                InnerOp::OpenLink(id, spawner, cascade) => self.open_link(id, spawner, cascade),
                InnerOp::Close() => self.close(),
                InnerOp::CloseLink(id) => self.close_link(id),
            }
        }
    }
}

/// Reactors get spawned with tokio, they only read from their channel and act on the messages
/// They reduce over an OperationStream
impl<S, K, M> Future for Reactor<S, K, M>
where
    S: Unpin + ReactorState<K, M>,
    K: Hash + Eq + 'static + Unpin,
{
    type Output = ();

    /// Handles on message at a time, clearing the inner ops queue every time
    /// This opens/closes links and has to be up to date at all times
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);

        loop {
            match ready!(Stream::poll_next(Pin::new(&mut this.channels.1), ctx)) {
                None => break,
                Some(item) => match item {
                    Operation::InternalMessage(id, msg, target) => {
                        this.handle_internal_msg(id, msg, target)
                    }
                    Operation::ExternalMessage(target, id, msg) => {
                        this.handle_external_msg(target, id, msg)
                    }
                    Operation::CloseLink(id) => this.close_link(id),
                    Operation::Close() => this.close(),
                    _ => unimplemented!(),
                },
            }

            while let Some(op) = this.inner_ops.pop_back() {
                match op {
                    InnerOp::OpenLink(id, spawner, cascade) => this.open_link(id, spawner, cascade),
                    InnerOp::Close() => this.close(),
                    InnerOp::CloseLink(id) => this.close_link(id),
                }
            }
        }

        info!(name = S::NAME, id = %this.id, "Reactor finished");
        return Poll::Ready(());
    }
}
