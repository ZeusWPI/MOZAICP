use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use std::any;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

mod message;
pub use self::message::Message;
mod types;
// ! Just some types to make things organised
pub use self::types::ReactorID;

// ANCHOR Traits
///
/// Main handler trait
/// This should apply one message to S
/// And use handle specific functions
///
pub trait Handler<S, C, M> {
    fn handle(&mut self, c: Context<S, C>, m: &mut M);
}

///
/// Context bundles a state and a handle
/// That handle is usually used to send new messages
///
pub struct Context<'a, S, C> {
    state: &'a mut S,
    handle: &'a mut C,
}

impl<'a, S, C> Context<'a, S, C> {
    fn split(self) -> (&'a mut S, &'a mut C) {
        (self.state, self.handle)
    }
}

///
/// The actual messages that are sent
/// These get consumed by the reactors
///
enum Operation<K, M> {
    InternalMessage(K, M),
    ExternalMessage(ReactorID, K, M),
    Close,
    // OpenLink
    // CloseLink
}

// ANCHOR Broker
///
/// A broker is nothing more then a map ReactorID -> ReactorChannel
/// So you can open links with just knowing a ReactorID
/// And spawn Reactors etc
///
struct Broker<K, M> {
    reactors: HashMap<ReactorID, mpsc::UnboundedSender<Operation<K, M>>>,
}

///
/// BrokerHandle wraps the Broker, for easy mutex manipulation
///
#[derive(Clone)]
pub struct BrokerHandle<K, M> {
    broker: Arc<Mutex<Broker<K, M>>>,
}

impl<K, M> BrokerHandle<K, M> {
    pub fn new() -> BrokerHandle<K, M> {
        let broker = Broker {
            reactors: HashMap::new(),
        };

        BrokerHandle {
            broker: Arc::new(Mutex::new(broker)),
        }
    }

    pub fn spawn(&self, _params: u32) -> ReactorID {
        unimplemented!()
    }

    pub fn remove(&self, params: &ReactorID) {
        let mut broker = self.broker.lock().unwrap();
        broker.reactors.remove(&params);
    }
}

// ANCHOR Reactor
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
    state: S,
    msg_handlers: HashMap<K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>>,

    tx: mpsc::UnboundedSender<Operation<K, M>>,
    rx: mpsc::UnboundedReceiver<Operation<K, M>>,
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
{
    pub fn new(state: S, id: ReactorID) -> Self {
        let (tx, rx) = mpsc::unbounded();

        Reactor {
            id,
            state,

            msg_handlers: HashMap::new(),
            tx,
            rx,
        }
    }

    pub fn add_handler<H>(&mut self, handler: H)
    where
        H: Into<(K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.msg_handlers.insert(id, handler);
    }

    pub fn get_handle(&self) -> ReactorHandle<K, M> {
        ReactorHandle {
            chan: self.tx.clone(),
        }
    }
}

/// Reactors get spawned with tokio, they only read from their channel and act on the messages
/// They reduce over an OperationStream
impl<S, K, M> Future for Reactor<S, K, M>
where
    K: Hash + Eq,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.rx.poll()) {
                None => return Ok(Async::Ready(())),
                Some(item) => match item {
                    Operation::InternalMessage(id, mut msg) => {
                        let mut handle = self.get_handle();

                        let ctx = Context {
                            state: &mut self.state,
                            handle: &mut handle,
                        };

                        self.msg_handlers
                            .get_mut(&id)
                            .map(|h| h.handle(ctx, &mut msg));
                    }

                    Operation::ExternalMessage(_reactor, _id, _msg) => {
                        unimplemented!();
                    }

                    Operation::Close => return Ok(Async::Ready(())),
                },
            }
        }
    }
}

///
/// ReactorHandle wraps a channel to send operations to the reactor
///
pub struct ReactorHandle<K, M> {
    chan: mpsc::UnboundedSender<Operation<K, M>>,
}

/// A context with a ReactorHandle is a ReactorContext
type ReactorContext<'a, S, K, M> = Context<'a, S, ReactorHandle<K, M>>;

///
/// FunctionHandler<S, T, F, R> makes a Handler from a function
/// For Messages that is
///
pub struct FunctionHandler<S, T, F, R> {
    phantom: PhantomData<(S, T, R)>,
    function: F,
}

impl<S, T, F, R> FunctionHandler<S, T, F, R>
where
    F: Fn(&mut S, &mut R, &T) -> () + Send,
    T: 'static,
{
    pub fn from(function: F) -> Box<Self> {
        Box::new(Self {
            phantom: PhantomData,
            function,
        })
    }
}

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
            .unbounded_send(Operation::Close)
            .expect("Couldn't close");
    }

    pub fn spawn(&mut self, _params: u32) {
        unimplemented!();
    }
}

impl<S, F, T, R> Into<(any::TypeId, Box<dyn Handler<S, R, Message> + Send>)>
    for Box<FunctionHandler<S, T, F, R>>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    T: 'static + Send,
    S: 'static + Send,
    R: 'static + Send,
{
    fn into(self) -> (any::TypeId, Box<dyn Handler<S, R, Message> + Send>) {
        let id = any::TypeId::of::<T>();
        (id, self)
    }
}

//
// Somewhere I want to make Message generic, but that wouldn't be useful
// You would need a TryBorrow trait to go from &'a mut M -> Option<&'a T>
// But to implement this trait you would need to be able to do that for every T
// Which only can with this Message type, or equivalents
//
// pub trait TryBorrow {
//     fn borrow<'a, T: 'static>(&'a mut self) -> Option<&'a T>;
// }
//
// And I don't know if you give this trait a trait to only try to be applied to special T's
// Like a T: FromPointerReader in capnproto's case
//

impl<S, T, F, R> Handler<S, R, Message> for FunctionHandler<S, T, F, R>
where
    F: Fn(&mut S, &mut R, &T) -> () + Send,
    T: 'static,
    R: 'static + Send,
{
    fn handle(&mut self, c: Context<S, R>, message: &mut Message) {
        let (state, handle) = c.split();
        message
            .borrow()
            .map(|item| (self.function)(state, handle, item))
            .expect("No message found at pointer location");
    }
}
