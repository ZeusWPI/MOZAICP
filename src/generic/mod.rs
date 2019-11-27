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

/// (SourceHandle, TargetHandle, SourceId, TargetID)
/// This is used to not expose Operation
type Handles<K, M> = (Sender<K, M>, Sender<K, M>, ReactorID, ReactorID);

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
/// Reactor channel is a struct that represents a reactor,
/// this reactor may not have spawned yet, see the ToConnect variant
///
enum ReactorChannel<K, M> {
    Connected(Sender<K, M>),
    ToConnect(Sender<K, M>, Receiver<K, M>),
}

///
/// A broker is nothing more then a map ReactorID -> ReactorChannel
/// So you can open links with just knowing a ReactorID
/// And spawn Reactors etc
///
struct Broker<K, M> {
    reactors: HashMap<ReactorID, ReactorChannel<K, M>>,
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
    /// Creates a new broker
    pub fn new() -> BrokerHandle<K, M> {
        let broker = Broker {
            reactors: HashMap::new(),
        };

        BrokerHandle {
            broker: Arc::new(Mutex::new(broker)),
        }
    }

    /// Removes a perticular reactor
    pub fn remove(&self, params: &ReactorID) {
        let mut broker = self.broker.lock().unwrap();
        broker.reactors.remove(&params);
    }

    /// Returns a channel to send messages to a reactor,
    /// this reactor may not be spawned yet
    fn get(&self, id: &ReactorID) -> Sender<K, M> {
        let mut broker = self.broker.lock().unwrap();
        if let Some(item) = broker.reactors.get(id) {
            match item {
                ReactorChannel::Connected(sender) => sender.clone(),
                ReactorChannel::ToConnect(sender, _) => sender.clone(),
            }
        } else {
            let (tx, rx) = mpsc::unbounded();
            broker
                .reactors
                .insert(id.clone(), ReactorChannel::ToConnect(tx.clone(), rx));
            tx
        }
    }

    /// Tell the broker that this reactor is getting spawned,
    /// giving up ownership of the receiver side of the message channel
    fn connect(&self, id: ReactorID) -> Option<(Sender<K, M>, Receiver<K, M>)> {
        let mut broker = self.broker.lock().unwrap();

        let (channel, receiver) = if let Some(item) = broker.reactors.remove(&id) {
            match item {
                ReactorChannel::Connected(sender) => (ReactorChannel::Connected(sender), None),
                ReactorChannel::ToConnect(sender, receiver) => (
                    ReactorChannel::Connected(sender.clone()),
                    Some((sender, receiver)),
                ),
            }
        } else {
            let (tx, rx) = mpsc::unbounded();
            (ReactorChannel::Connected(tx.clone()), Some((tx, rx)))
        };

        broker.reactors.insert(id, channel);

        receiver
    }
}

impl<K, M> BrokerHandle<K, M>
where
    K: 'static + Eq + Hash + Send,
    M: 'static + Send,
{
    /// Spawns a perticular reactor
    pub fn spawn<S: 'static + Send + ReactorState<K, M>>(
        &self,
        params: CoreParams<S, K, M>,
        id: Option<ReactorID>,
    ) -> ReactorID {
        let id = id.unwrap_or_else(|| rand::random::<u64>().into());

        let mut reactor = Reactor::new(
            id,
            self.clone(),
            params,
            self.connect(id).expect("Already connected"),
        );

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
    pub fn from(function: F) -> Self {
        Self {
            phantom: PhantomData,
            function,
        }
    }
}

impl<F, S, R, T> Into<(any::TypeId, Self)> for FunctionHandler<F, S, R, T>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send,
{
    fn into(self) -> (any::TypeId, Self) {
        (any::TypeId::of::<T>(), self)
    }
}

///
/// This is just stupid, you shouldn't have to implement Handler for ReactorHandle and LinkHandle
/// but this for<'b> is fucking the compiler up.
///
/// As long as no fix is found, this will have to do
/// It is just stupid for every M type, here Message you would have to implement it twice
/// though it is the same implementation
///
/// For clarification, this implementation goes from a generic Message
/// to a specific T that is expected for F
///
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
