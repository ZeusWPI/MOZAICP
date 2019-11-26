use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use super::*;

macro_rules! reactorHandle {
    ($e:expr) => {
        ReactorHandle {
            chan: &$e.tx,
            id: &$e.id,
            inner_ops: &mut $e.inner_ops,
        };
    };
}

pub trait ReactorState<K, M> {
    fn init<'a>(&mut self, &mut ReactorHandle<'a, K, M>) {}
}

impl<K, M> ReactorState<K, M> for () {}

///
/// Reactor is the meat and the potatoes of MOZAIC
/// You can register Handers (just functions)
/// That are called when the correct data T is being handled
///
pub struct Reactor<S, K, M>
where
    K: Hash + Eq,
{
    id: ReactorID,

    broker: BrokerHandle<K, M>,
    state: S,
    msg_handlers: HashMap<K, Box<dyn for<'a> Handler<S, ReactorHandle<'a, K, M>, M> + Send>>,

    links: HashMap<
        ReactorID,
        Box<dyn for<'a, 'b> Handler<(), ReactorHandle<'b, K, M>, LinkOperation<'a, K, M>> + Send>,
    >,

    tx: Sender<K, M>,
    rx: Receiver<K, M>,

    inner_ops: VecDeque<InnerOp<K, M>>,
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
{
    pub fn new(
        id: ReactorID,
        broker: BrokerHandle<K, M>,
        params: CoreParams<S, K, M>,
        (tx, rx): (Sender<K, M>, Receiver<K, M>),
    ) -> Self {

        Reactor {
            id,
            broker,
            state: params.state,
            msg_handlers: params.handlers,
            links: HashMap::new(),
            tx,
            rx,
            inner_ops: VecDeque::new(),
        }
    }

    pub fn get_handle<'a>(&'a mut self) -> ReactorHandle<'a, K, M> {
        reactorHandle!(self)
    }

    fn handle_internal_msg(&mut self, id: K, mut msg: M) {
        println!("Handling internal message");

        let mut handle = reactorHandle!(self);

        if let Some(h) = self.msg_handlers.get_mut(&id) {
            h.handle(&mut self.state, &mut handle, &mut msg);
        }

        let mut m = LinkOperation::InternalMessage(&id, &mut msg);
        let mut state = ();
        for (_, link) in self.links.iter_mut() {
            println!("Handling internal message for link");
            link.handle(&mut state, &mut handle, &mut m);
        }
    }

    fn handle_external_msg(&mut self, target: ReactorID, id: K, mut msg: M) {
        println!("Handling external message");
        let mut handle = reactorHandle!(self);

        let mut m = LinkOperation::ExternalMessage(&id, &mut msg);

        self.links
            .get_mut(&target)
            .map(|h| h.handle(&mut (), &mut handle, &mut m))
            .expect("AAAAAAAAAAHHHHHHHHHHHH");
    }

    fn open_link(&mut self, target: ReactorID, spawner: LinkSpawner<K, M>) {
        println!("Opening link");
        let tx = self.broker.get(&target);
        let handles = (self.tx.clone(), tx, target);
        self.links.insert(target, spawner(handles));
    }

    fn close_link(&mut self, target: ReactorID) {
        self.links.remove(&target);
    }
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
    S: ReactorState<K, M>,
{
    pub fn init(&mut self) {
        let mut handle = reactorHandle!(self);

        self.state.init(&mut handle);
    }
}

/// Reactors get spawned with tokio, they only read from their channel and act on the messages
/// They reduce over an OperationStream
impl<S, K, M> Future for Reactor<S, K, M>
where
    K: Hash + Eq + 'static,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.rx.poll()) {
                None => return Ok(Async::Ready(())),
                Some(item) => match item {
                    Operation::InternalMessage(id, msg) => self.handle_internal_msg(id, msg),
                    Operation::ExternalMessage(target, id, msg) => {
                        self.handle_external_msg(target, id, msg)
                    }
                    _ => unimplemented!(),
                },
            }

            while let Some(op) = self.inner_ops.pop_back() {
                match op {
                    InnerOp::OpenLink(id, spawner) => self.open_link(id, spawner),
                    InnerOp::Close() => return Ok(Async::Ready(())),
                    InnerOp::CloseLink(id) => self.close_link(id),
                }
            }
        }
    }
}


pub enum InnerOp<K, M> {
    OpenLink(ReactorID, LinkSpawner<K, M>),
    CloseLink(ReactorID),
    Close(),
}

///
/// ReactorHandle wraps a channel to send operations to the reactor
///
// TODO: Only references please, this is shitty

pub struct ReactorHandle<'a, K, M> {
    chan: &'a Sender<K, M>,
    id: &'a ReactorID,
    inner_ops: &'a mut VecDeque<InnerOp<K, M>>,
}

impl<'a, K, M> ReactorHandle<'a, K, M>
where
    K: 'static + Eq + Hash + Send,
    M: 'static + Send,
{
    pub fn open_link<L>(&mut self, target: ReactorID, spawner: L)
    where
        L: Into<LinkSpawner<K, M>>,
    {
        self.inner_ops.push_back(
            InnerOp::OpenLink(target, spawner.into())
        );
    }

    pub fn close(&mut self) {
        self.inner_ops.push_back(
            InnerOp::Close(),
        );
    }

    pub fn spawn<S: 'static + Send>(&mut self, _params: CoreParams<S, K, M>) -> ReactorID {
        unimplemented!();
        // self.broker.spawn(params)
    }

    pub fn id(&mut self) -> &'a ReactorID {
        &self.id
    }
}

// ANCHOR Implementation with any::TypeId
/// To use MOZAIC a few things
/// Everything your handlers use, so the Context and how to get from M to T
/// This is already implemented for every T, with the use of Rusts TypeIds
/// But you may want to implement this again, with for example Capnproto messages
/// so you can send messages over the internet
impl<'a> ReactorHandle<'a, any::TypeId, Message> {
    pub fn send_internal<T: 'static>(&mut self, msg: T) {
        let id = any::TypeId::of::<T>();
        let msg = Message::from(msg);
        self.chan
            .unbounded_send(Operation::InternalMessage(id, msg))
            .expect("crashed");
    }
}

// ANCHOR Params
pub struct CoreParams<S, K, M> {
    state: S,
    handlers: HashMap<K, Box<dyn for<'a> Handler<S, ReactorHandle<'a, K, M>, M> + Send>>,
}

impl<S, K, M> CoreParams<S, K, M>
where
    K: Eq + Hash + 'static,
    M: 'static,
{
    pub fn new(state: S) -> Self {
        CoreParams {
            state,
            handlers: HashMap::new(),
        }
    }

    pub fn handler<H, J>(&mut self, handler: H)
    where
        H: Into<(K, J)>,
        J: for<'a> Handler<S, ReactorHandle<'a, K, M>, M> + Send + 'static,
    {
        let (id, handler) = handler.into();
        self.handlers.insert(id, Box::new(handler));
    }
}
