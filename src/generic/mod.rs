use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use rand;

use std::any;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

mod message;
pub use self::message::Message;
mod link;
mod reactor;
mod types;

pub use self::link::{Link, LinkHandle, LinkParams};
pub use self::reactor::{CoreParams, Reactor, ReactorHandle, ReactorState};

// ! Just some types to make things organised
pub use self::types::ReactorID;

pub struct Initialize();

/// Shortcut types
type Sender<K, M> = mpsc::UnboundedSender<Operation<K, M>>;
type Receiver<K, M> = mpsc::UnboundedReceiver<Operation<K, M>>;

// ANCHOR Traits
///
/// Main handler trait
/// This should apply one message to S
/// And use handle specific functions
///
pub trait Handler<S, H, M> {
    fn handle(&mut self, s: &mut S, h: &mut H, m: &mut M);
}

// ///
// /// Context bundles a state and a handle
// /// That handle is usually used to send new messages
// ///
// pub struct Context<'a, S, C> {
//     state: &'a mut S,
//     handle: &'a mut C,
// }

// impl<'a, S, C> Context<'a, S, C> {
//     fn split(self) -> (&'a mut S, &'a mut C) {
//         (self.state, self.handle)
//     }
// }

/// (SourceHandle, TargetHandle)
type Handles<K, M> = (Sender<K, M>, Sender<K, M>, ReactorID);
pub type LinkSpawner<K, M> = Box<
    dyn FnOnce(
            Handles<K, M>,
        ) -> Box<
            dyn for<'a, 'b> Handler<(), ReactorHandle<'b, K, M>, LinkOperation<'a, K, M>> + Send,
        > + Send,
>;

pub enum LinkOperation<'a, K, M> {
    InternalMessage(&'a K, &'a mut M),
    ExternalMessage(&'a K, &'a mut M),
}

///
/// The actual messages that are sent
/// These get consumed by the reactors
///
pub enum Operation<K, M> {
    InternalMessage(K, M),
    ExternalMessage(ReactorID, K, M),
    Close(),
    OpenLink(ReactorID, LinkSpawner<K, M>),
    CloseLink(ReactorID),
}

// ANCHOR Broker
///
/// A broker is nothing more then a map ReactorID -> ReactorChannel
/// So you can open links with just knowing a ReactorID
/// And spawn Reactors etc
///
struct Broker<K, M> {
    reactors: HashMap<ReactorID, Sender<K, M>>,
}

///
/// BrokerHandle wraps the Broker, for easy mutex manipulation
///
pub struct BrokerHandle<K, M> {
    broker: Arc<Mutex<Broker<K, M>>>,
}

impl<K, M> Clone for BrokerHandle<K, M> {
    fn clone(&self) -> Self {
        BrokerHandle {
            broker: self.broker.clone(),
        }
    }
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

    pub fn remove(&self, params: &ReactorID) {
        let mut broker = self.broker.lock().unwrap();
        broker.reactors.remove(&params);
    }

    fn get(&self, id: &ReactorID) -> Option<Sender<K, M>> {
        self.broker.lock().unwrap().reactors.get(id).cloned()
    }
}

impl<K, M> BrokerHandle<K, M>
where
    K: 'static + Eq + Hash + Send,
    M: 'static + Send,
{
    pub fn spawn<S: 'static + Send + ReactorState<K, M>>(&self, params: CoreParams<S, K, M>, id: Option<ReactorID>) -> ReactorID {
        let id = id.unwrap_or_else(|| rand::random::<u64>().into());
        println!("Spawning {:?}", id);
        let (mut reactor, sender) = Reactor::new(id, self.clone(), params);

        self.broker
            .lock()
            .unwrap()
            .reactors
            .insert(id.clone(), sender);


        reactor.init();
        tokio::spawn(reactor);

        id
    }
}

///
/// FunctionHandler<S, T, F, R> makes a Handler from a function
/// For Messages that is
///
pub struct FunctionHandler<F, S, R, T>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send,
{
    phantom: PhantomData<(S, R, T)>,
    function: F,
}

impl<F, S, R, T> FunctionHandler<F, S, R, T>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send,
{
    pub fn from(function: F) -> Box<Self> {
        Box::new(Self {
            phantom: PhantomData,
            function,
        })
    }
}

// impl<F, S, T> From<FunctionHandler<F, S, ReactorHandle<'_, any::TypeId, Message>, T>> for (any::TypeId, Box<dyn Handler<S, ReactorHandle<'_, any::TypeId, Message>, Message> + Send>)
// where
// F: 'static + Send + Fn(&mut S, &mut ReactorHandle<'_, any::TypeId, Message>, &T) -> (),
// S: 'static + Send,
// T: 'static + Send,
// {
//     fn from(other: FunctionHandler<F, S, ReactorHandle<'_, any::TypeId, Message>, T>) -> Self {
//         let id = any::TypeId::of::<T>();
//         (id, Box::new(other))
//     }
// }

// impl<F, S, H, T>
//     Into<(
//         any::TypeId,
//         Box<(dyn Handler<S, H, Message> + Send + 'static)>,
//     )> for FunctionHandler<F, S, H, T>
// where
//     F: 'static + Send + Fn(&mut S, &mut H, &T) -> (),
//     S: 'static + Send,
//     H: 'static + Send,
//     T: 'static + Send,
// {
//     fn into(
//         self,
//     ) -> (
//         any::TypeId,
//         Box<(dyn for<'a> Handler<S, H, Message> + Send + 'static)>,
//     ) {
//         let id = any::TypeId::of::<T>();
//         (id, Box::new(self))
//     }
// }

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

impl<'a, K, F, S, T> Handler<S, ReactorHandle<'a, K, Message>, Message>
    for FunctionHandler<F, S, ReactorHandle<'_, K, Message>, T>
where
    F: 'static + Send + for<'b> Fn(&mut S, &mut ReactorHandle<'b, K, Message>, &T) -> (),
    S: 'static + Send,
    T: 'static + Send,
    K: 'static + Send,
{
    fn handle<'b>(
        &mut self,
        state: &mut S,
        handle: &mut ReactorHandle<'b, K, Message>,
        message: &mut Message,
    ) {
        message
            .borrow()
            .map(|item| (self.function)(state, handle, item))
            .expect("No message found at pointer location");
    }
}

impl<'a, K, F, S, T> Handler<S, LinkHandle<'a, K, Message>, Message>
    for FunctionHandler<F, S, LinkHandle<'_, K, Message>, T>
where
    F: 'static + Send + for<'b> Fn(&mut S, &mut LinkHandle<'b, K, Message>, &T) -> (),
    S: 'static + Send,
    T: 'static + Send,
    K: 'static + Send,
{
    fn handle<'b>(
        &mut self,
        state: &mut S,
        handle: &mut LinkHandle<'b, K, Message>,
        message: &mut Message,
    ) {
        message
            .borrow()
            .map(|item| (self.function)(state, handle, item))
            .expect("No message found at pointer location");
    }
}


// impl<F, S, H, T> Handler<S, H, Message>
//     for FunctionHandler<F, S, H, T>
// where
//     F: 'static + Send + for<'a> Fn(&mut S, &mut H, &T) -> (),
//     S: 'static + Send,
//     T: 'static + Send,
//     H: 'static + Send,
// {
//     fn handle(
//         &mut self,
//         state: &mut S,
//         handle: &mut H,
//         message: &mut Message,
//     ) {
//         message
//             .borrow()
//             .map(|item| (self.function)(state, handle, item))
//             .expect("No message found at pointer location");
//     }
// }
