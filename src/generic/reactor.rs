use std::collections::HashMap;
use std::hash::Hash;

use super::*;

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
    msg_handlers: HashMap<K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>>,


    links: HashMap<ReactorID, Box<dyn Handler<(), ReactorHandle<K, M>, LinkOperation<K, M>> + Send>>,

    tx: Sender<K, M>,
    rx: Receiver<K, M>,
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
{
    pub fn new(id: ReactorID, broker: BrokerHandle<K, M>, params: CoreParams<S, K, M>) -> Self {
        let (tx, rx) = mpsc::unbounded();

        Reactor {
            id,
            broker,
            state: params.state,
            msg_handlers: params.handlers,
            links: HashMap::new(),
            tx,
            rx,
        }
    }

    pub fn get_handle(&self) -> ReactorHandle<K, M> {
        ReactorHandle {
            chan: self.tx.clone(),
            broker: self.broker.clone(),
        }
    }

    fn handle_internal_msg(&mut self, id: K, mut msg: M) {
        let mut handle = self.get_handle();

        let ctx = Context {
            state: &mut self.state,
            handle: &mut handle,
        };

        self.msg_handlers
            .get_mut(&id)
            .map(|h| h.handle(ctx, &mut msg));
    }

    fn open_link(&mut self, target: ReactorID, spawner: LinkSpawner<K, M>) {

        if let Some(tx) = self.broker.get(&target) {
            let handles = (
                self.tx.clone(),
                tx,
            );
            self.links.insert(target, spawner(handles));
        } else {
            eprintln!("No such reactor");
        }
    }

    fn close_link(&mut self, target: ReactorID) {
        self.links.remove(&target);
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
                    Operation::Close() => return Ok(Async::Ready(())),
                    Operation::OpenLink(id, spawner) => self.open_link(id, spawner),
                    Operation::CloseLink(id) => self.close_link(id),

                    _ => {
                        unimplemented!();
                    }
                },
            }
        }
    }
}

///
/// ReactorHandle wraps a channel to send operations to the reactor
///
// TODO: Only references please, this is shitty
pub struct ReactorHandle<K, M> {
    chan: Sender<K, M>,
    broker: BrokerHandle<K, M>,
}

/// A context with a ReactorHandle is a ReactorContext
type ReactorContext<'a, S, K, M> = Context<'a, S, ReactorHandle<K, M>>;

// ANCHOR Implementation with any::TypeId
/// To use MOZAIC a few things
/// Everything your handlers use, so the Context and how to get from M to T
/// This is already implemented for every T, with the use of Rusts TypeIds
/// But you may want to implement this again, with for example Capnproto messages
/// so you can send messages over the internet
impl ReactorHandle<any::TypeId, Message> {
    pub fn open_link(&mut self) {
        unimplemented!();
    }

    pub fn send_internal<T: 'static>(&mut self, msg: T) {
        let id = any::TypeId::of::<T>();
        let msg = Message::from(msg);
        self.chan
            .unbounded_send(Operation::InternalMessage(id, msg))
            .expect("crashed");
    }

    pub fn close(&mut self) {
        self.chan
            .unbounded_send(Operation::Close())
            .expect("Couldn't close");
    }

    pub fn spawn(&mut self, _params: u32) {
        unimplemented!();
    }
}


// ANCHOR Params
pub struct CoreParams<S, K, M> {
    state: S,
    handlers: HashMap<K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>>,
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

    pub fn handler<H>(&mut self, handler: H)
    where
        H: Into<(K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.handlers.insert(id, handler);
    }
}
