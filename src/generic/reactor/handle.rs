use super::InnerOp;
use crate::generic::{
    BrokerHandle, CoreParams, IntoMessage, LinkSpawner, Operation, ReactorID, ReactorState, Sender,
    TargetReactor, SenderHandle,
};

use std::collections::VecDeque;
use std::hash::Hash;

/// Handle to the reactor, managing operation and messages
pub struct ReactorHandle<'a, K, M> {
    chan: &'a Sender<K, M>,
    id: &'a ReactorID,
    inner_ops: &'a mut VecDeque<InnerOp<K, M>>,
    broker: &'a mut BrokerHandle<K, M>,
}

impl<'a, K, M> ReactorHandle<'a, K, M> {
    pub fn new(
        chan: &'a Sender<K, M>,
        id: &'a ReactorID,
        inner_ops: &'a mut VecDeque<InnerOp<K, M>>,
        broker: &'a mut BrokerHandle<K, M>,
    ) -> Self {
        ReactorHandle {
            chan,
            id,
            inner_ops,
            broker,
        }
    }
}

impl<'a, K, M> ReactorHandle<'a, K, M>
where
    K: 'static + Send + Eq + Hash + Unpin,
    M: 'static + Send,
{
    pub fn open_link<L>(&mut self, target: ReactorID, spawner: L, cascade: bool)
    where
        L: Into<LinkSpawner<K, M>>,
    {
        self.inner_ops
            .push_back(InnerOp::OpenLink(target, spawner.into(), cascade));
    }

    pub fn open_reactor_like(&mut self, target: ReactorID, tx: Sender<K, M>) {
        self.broker.spawn_reactorlike(target, tx);
    }

    pub fn close(&mut self) {
        self.inner_ops.push_back(InnerOp::Close());
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

    pub fn chan(&self) -> SenderHandle<K, M> {
        SenderHandle { sender: self.chan.clone() }
    }
}

// ANCHOR Implementation with any::TypeId
/// Generic implementation of reactor handle, this one is able to handle every T
/// Making it generic by forming a Message and sending it through
///
/// You would want to implement this again with Capnproto messages
/// to be able to send them over the internet
impl<'a, K, M> ReactorHandle<'a, K, M> {
    pub fn send_internal<T: 'static + IntoMessage<K, M>>(&mut self, msg: T, to: TargetReactor) {
        if let Some((id, msg)) = T::into_msg(msg) {
            self.chan
                .unbounded_send(Operation::InternalMessage(id, msg, to))
                .expect("crashed");
        }
    }
}
