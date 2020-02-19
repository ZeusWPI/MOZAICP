use super::{Closer, LinkHandle, LinkParams};
use crate::generic::{Handler, LinkOperation, Operation, ReactorHandle, ReactorID, Sender};

use std::collections::HashMap;
use std::hash::Hash;

/// Macro to create link handles
/// This does not borrow the entire Link like a function would
macro_rules! linkHandle {
    ($e:expr) => {
        LinkHandle::new(&$e.link_state)
    };
}

/// Package channels and id's neatly together
pub struct LinkState<K, M> {
    pub source: Sender<K, M>,
    pub target: Sender<K, M>,
    pub source_id: ReactorID,
    pub target_id: ReactorID,
}

/// A link pair links 2 reactors together
/// Which enables them to communicate with messages
pub struct Link<S, K, M> {
    state: S,

    internal_handlers:
        HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,
    external_handlers:
        HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,

    link_state: LinkState<K, M>,
    closer: Closer<S, K, M>,
}

impl<S, K, M> Link<S, K, M> {
    pub fn new(link_state: LinkState<K, M>, params: LinkParams<S, K, M>) -> Self {
        let (state, internal_handlers, external_handlers, closer) = params.consume();
        Self {
            link_state,
            state,
            internal_handlers,
            external_handlers,
            closer,
        }
    }
}

/// A link is a handler without a real state that handles LinkOperations
/// These link operations are incomming messages
impl<'a, 'b, S, K, M> Handler<(), ReactorHandle<'b, K, M>, &'a mut LinkOperation<'a, K, M>>
    for Link<S, K, M>
where
    K: Hash + Eq,
{
    fn handle(
        &mut self,
        _: &mut (),
        _handle: &mut ReactorHandle<'b, K, M>,
        m: &mut LinkOperation<K, M>,
    ) {
        match m {
            LinkOperation::InternalMessage(id, message) => {
                if let Some(h) = self.internal_handlers.get_mut(id) {
                    h.handle(&mut self.state, &mut linkHandle!(self), (id, message));
                } else {
                    trace!("No handler found");
                }
            }
            LinkOperation::ExternalMessage(id, message) => {
                if let Some(h) = self.external_handlers.get_mut(id) {
                    h.handle(&mut self.state, &mut linkHandle!(self), (id, message));
                } else {
                    trace!("No handler found");
                }
            }
            LinkOperation::Close() => {
                (self.closer)(&mut self.state, &mut linkHandle!(self));
                if let Result::Err(_) = self
                    .link_state
                    .target
                    .unbounded_send(Operation::CloseLink(self.link_state.source_id))
                {
                    // The problem is this doesn't happen always
                    trace!("Cannot send close message, channel closed");
                }
            }
        };
    }
}
