use super::LinkState;

use crate::generic::{IntoMessage, Operation, ReactorID, TargetReactor};

/// Handle to manipulate a link
/// Being able so send new messages and close the link
#[derive(Clone)]
pub struct LinkHandle<'a, K, M> {
    state: &'a LinkState<K, M>,
}

impl<'a, K, M> LinkHandle<'a, K, M> {
    pub fn new(state: &'a LinkState<K, M>) -> Self {
        Self { state }
    }
    pub fn send_message<T: 'static + IntoMessage<K, M>>(&mut self, msg: T) {
        if let Some((id, msg)) = T::into_msg(msg) {
            self.send_raw(id, msg);
        }
    }

    pub fn send_internal<T: 'static + IntoMessage<K, M>>(&mut self, msg: T, target: TargetReactor) {
        if let Some((id, msg)) = T::into_msg(msg) {
            self.send_internal_raw(id, msg, target);
        }
    }

    pub fn send_raw(&mut self, id: K, msg: M) {
        if self
            .state
            .target
            .unbounded_send(Operation::ExternalMessage(
                self.state.source_id.clone(),
                id,
                msg,
            ))
            .is_err()
        {
            if self
                .state
                .source
                .unbounded_send(Operation::CloseLink(self.state.target_id))
                .is_err()
            {
                trace!("Internal reactor is already closed, nothing to do.");
            }
        }
    }

    pub fn send_internal_raw(&mut self, id: K, msg: M, target: TargetReactor) {
        if self
            .state
            .source
            .unbounded_send(Operation::InternalMessage(id, msg, target))
            .is_err()
        {
            trace!("Internal reactor is already closed, nothing to do.");
        }
    }

    pub fn close_link(&mut self) {
        if self
            .state
            .source
            .unbounded_send(Operation::CloseLink(self.state.target_id))
            .is_err()
        {
            trace!("Cannot closed link, link already closed");
        }
    }

    pub fn target_id(&'a self) -> &'a ReactorID {
        &self.state.target_id
    }

    pub fn source_id(&'a self) -> &'a ReactorID {
        &self.state.source_id
    }
}
