use super::{CoreParams, Reactor, ReactorID, ReactorState, Receiver, Sender, SenderHandle};

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::future::{FutureExt, RemoteHandle, Future};
use futures::stream::StreamExt;
use futures::task::SpawnExt;

use tracing_futures::Instrument;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

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
    tx: mpsc::UnboundedSender<RemoteHandle<()>>,
}

impl<K, M> Clone for BrokerHandle<K, M> {
    fn clone(&self) -> Self {
        BrokerHandle {
            broker: self.broker.clone(),
            pool: self.pool.clone(),
            tx: self.tx.clone(),
        }
    }
}

impl<K, M> BrokerHandle<K, M> {
    /// Creates a new broker
    pub fn new(pool: ThreadPool) -> (Self, RemoteHandle<()>) {
        let (tx, mut rx) = mpsc::unbounded();

        let fut = async move {
            loop {
                if let Some(next) = rx.next().await {
                    next.await;
                } else {
                    break;
                }
            }
            Some(())
        };

        let handle = pool.spawn_with_handle(fut.map(|_| info!("Broker finished") )).unwrap();

        let broker = Broker {
            reactors: HashMap::new(),
        };

        (
            BrokerHandle {
                broker: Arc::new(Mutex::new(broker)),
                pool,
                tx,
            },
            handle,
        )
    }

    pub fn spawn_fut<O, Fut: Future<Output=O> + Send +'static>(&self, id: ReactorID, name: &str, fut: Fut) {
        info!(%id, "Start Reactor");
        graph::add_node(&id, name);

        let handle = self.pool.spawn_with_handle(fut.map(move |_| {
            graph::remove_node(&id);
            info!(%id, "Closed Reactor");
        })).unwrap();

        self.tx.unbounded_send(handle);
    }

    /// Removes a perticular reactor
    // pub fn remove(&self, id: &ReactorID) {
    //     let mut broker = self.broker.lock().unwrap();
    //     broker.reactors.remove(&id);
    // }

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

    pub fn spawn_reactorlike<O, Fut: Future<Output=O> + Send +'static>(&self, id: ReactorID, sender: Sender<K, M>, fut: Fut, name: &str) {
        self.set(id, sender);
        self.spawn_fut(id, name, fut);
    }
}

use crate::graph;
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
        let id = id.unwrap_or_else(|| ReactorID::rand());

        let mut reactor = Reactor::new(
            id,
            self.clone(),
            params,
            self.connect(id).expect("Already connected"),
        );

        reactor.init();

        self.spawn_fut(id, S::NAME, reactor.instrument(trace_span!("Reactor", name = S::NAME, %id)));

        id
    }

    pub fn get_sender(&self, target: &ReactorID) -> SenderHandle<K, M> {
        SenderHandle {
            sender: self.get(target),
        }
    }
}
