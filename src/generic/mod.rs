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
mod reactor;
mod link;

pub use self::reactor::{Reactor, ReactorHandle};
pub use self::link::{Link, LinkHandle};

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
    Close(),
    OpenLink(ReactorID),
    CloseLink(ReactorID),
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

/// Shortcut types
type Sender<K, M> = mpsc::UnboundedSender<Operation<K, M>>;
type Receiver<K, M> = mpsc::UnboundedReceiver<Operation<K, M>>;

///
/// FunctionHandler<S, T, F, R> makes a Handler from a function
/// For Messages that is
///
pub struct FunctionHandler<F, S, R, T> {
    phantom: PhantomData<(S, R, T)>,
    function: F,
}

impl<F, S, R, T> FunctionHandler<F, S, R, T>
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

impl<F, S, R, T> Into<(any::TypeId, Box<dyn Handler<S, R, Message> + Send>)>
    for Box<FunctionHandler<F, S, R, T>>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send,
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

impl<F, S, R, T> Handler<S, R, Message> for FunctionHandler<F, S, R, T>
where
    F: Fn(&mut S, &mut R, &T) -> () + Send,
    R: 'static + Send,
    T: 'static,
{
    fn handle(&mut self, c: Context<S, R>, message: &mut Message) {
        let (state, handle) = c.split();
        message
            .borrow()
            .map(|item| (self.function)(state, handle, item))
            .expect("No message found at pointer location");
    }
}
