use crate::generic::{
    BrokerHandle, CoreParams, IntoMessage, LinkSpawner, Operation, ReactorID, ReactorState, Sender,
    SenderHandle, TargetReactor,
};

use std::hash::Hash;

/// Handle to the reactor, managing operation and messages
pub struct ReactorHandle<'a, K, M> {
    chan: &'a Sender<K, M>,
    id: &'a ReactorID,
    broker: &'a mut BrokerHandle<K, M>,
}

impl<'a, K, M> ReactorHandle<'a, K, M> {
    pub fn new(
        chan: &'a Sender<K, M>,
        id: &'a ReactorID,
        broker: &'a mut BrokerHandle<K, M>,
    ) -> Self {
        ReactorHandle {
            chan,
            id,
            broker,
        }
    }
}

use futures::future::Future;
impl<'a, K, M> ReactorHandle<'a, K, M>
where
    K: 'static + Send + Eq + Hash + Unpin,
    M: 'static + Send,
{
    pub fn open_link<L>(&mut self, target: ReactorID, spawner: L, cascade: bool)
    where
        L: Into<LinkSpawner<K, M>>,
    {
        if self.chan.unbounded_send(Operation::OpenLink(target, spawner.into(), cascade)).is_err() {
            info!("Couldn't send open link operation");
        }
    }

    pub fn open_reactor_like<O, Fut: Future<Output = O> + Send + 'static>(
        &mut self,
        target: ReactorID,
        tx: Sender<K, M>,
        fut: Fut,
        name: &str,
    ) {
        self.broker.spawn_reactorlike(target, tx, fut, name);
    }

    pub fn close(&mut self) {
        if self.chan.unbounded_send(Operation::Close()).is_err() {
            info!("Couldn't send close operation");
        }
    }

    pub fn spawn<S: 'static + Send + ReactorState<K, M> + Unpin>(
        &mut self,
        params: CoreParams<S, K, M>,
        id: Option<ReactorID>,
    ) -> ReactorID {
        self.broker.spawn(params, id)
    }

    pub fn id(&mut self) -> &'a ReactorID {
        &self.id
    }

    pub fn get(&self, id: &ReactorID) -> SenderHandle<K, M> {
        self.broker.get_sender(id)
    }

    pub fn chan(&self) -> SenderHandle<K, M> {
        SenderHandle {
            sender: self.chan.clone(),
        }
    }
}

/// Generic implementation of reactor handle, this one is able to handle every T
/// Making it generic by forming a Message and sending it through
///
/// You would want to implement this again with Capnproto messages
/// to be able to send them over the internet
impl<'a, K, M> ReactorHandle<'a, K, M> {
    pub fn send_internal<T: 'static + IntoMessage<K, M>>(&mut self, msg: T, to: TargetReactor) {
        if let Some((id, msg)) = T::into_msg(msg) {
            if self
                .chan
                .unbounded_send(Operation::InternalMessage(id, msg, to))
                .is_err()
            {
                trace!("Internal reactor is already closed, nothing to do");
            }
        }
    }
}
