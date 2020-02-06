use super::{Sender, Receiver, ReactorID, CoreParams, ReactorState, SenderHandle, Reactor};

use futures::future::RemoteHandle;
use futures::executor::ThreadPool;
use futures::channel::mpsc;
use futures::task::SpawnExt;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, Arc};

///
/// Reactor channel is an enum that represents a reactor,
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
    pool: ThreadPool,
}

impl<K, M> Clone for BrokerHandle<K, M> {
    fn clone(&self) -> Self {
        BrokerHandle {
            broker: self.broker.clone(),
            pool: self.pool.clone(),
        }
    }
}

impl<K, M> BrokerHandle<K, M> {
    /// Creates a new broker
    pub fn new(pool: ThreadPool) -> BrokerHandle<K, M> {
        let broker = Broker {
            reactors: HashMap::new(),
        };

        BrokerHandle {
            broker: Arc::new(Mutex::new(broker)),
            pool,
        }
    }

    /// Removes a perticular reactor
    pub fn remove(&self, id: &ReactorID) {
        let mut broker = self.broker.lock().unwrap();
        broker.reactors.remove(&id);
    }

    /// Returns a channel to send messages to a reactor,
    /// this reactor may not be spawned yet
    pub fn get(&self, id: &ReactorID) -> Sender<K, M> {
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
                ReactorChannel::Connected(_sender) => return None,
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

    fn set(&self, id: ReactorID, sender: Sender<K, M>) {
        let mut broker = self.broker.lock().unwrap();

        broker
            .reactors
            .insert(id, ReactorChannel::Connected(sender));
    }

    pub fn spawn_reactorlike(&self, id: ReactorID, sender: Sender<K, M>) {
        self.set(id, sender);
    }
}

impl<K, M> BrokerHandle<K, M>
where
    K: 'static + Eq + Hash + Send + Unpin,
    M: 'static + Send,
{
    /// Spawns a perticular reactor
    pub fn spawn<S: 'static + Send + ReactorState<K, M> + Unpin>(
        &self,
        params: CoreParams<S, K, M>,
        id: Option<ReactorID>,
    ) -> ReactorID {
        let (handle, id) = self.spawn_with_handle(params, id);
        handle.forget();
        id
    }

    pub fn spawn_with_handle<S: 'static + Send + ReactorState<K, M> + Unpin>(
        &self,
        params: CoreParams<S, K, M>,
        id: Option<ReactorID>,
    ) -> (RemoteHandle<()>, ReactorID) {
        let id = id.unwrap_or_else(|| ReactorID::rand());

        let mut reactor = Reactor::new(
            id,
            self.clone(),
            params,
            self.connect(id).expect("Already connected"),
        );

        reactor.init();

        let fut = self
            .pool
            .spawn_with_handle(reactor)
            .expect("Couldn't spawn reactor");

        (fut, id)
    }

    pub fn get_sender(&self, target: &ReactorID) -> SenderHandle<K, M> {
        SenderHandle { sender: self.get(target) }
    }
}
