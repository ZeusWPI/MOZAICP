use super::{Closer, LinkState, Link};
use crate::generic::{Handler, LinkHandle, LinkSpawner};

use std::collections::HashMap;
use std::hash::Hash;

type HandlersMap<S, K, M> = HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>;

// ANCHOR Params
/// Builder pattern for constructing links
pub struct LinkParams<S, K, M> {
    state: S,
    internal_handlers: HandlersMap<S, K, M>,
    external_handlers: HandlersMap<S, K, M>,
    closer: Closer<S, K, M>,
}

impl<S, K, M> LinkParams<S, K, M> {
    pub fn consume(self) -> (S, HandlersMap<S, K, M>, HandlersMap<S, K, M>, Closer<S, K, M>) {
        (
            self.state, self.internal_handlers, self.external_handlers, self.closer
        )
    }
}

impl<S, K, M> LinkParams<S, K, M>
where
    K: Eq + Hash,
{
    pub fn new(state: S) -> Self {
        Self {
            state,
            internal_handlers: HashMap::new(),
            external_handlers: HashMap::new(),
            closer: Box::new(|_, _| {}),
        }
    }

    pub fn closer<F>(mut self, close_f: F) -> Self
        where F: 'static + Send + for<'a> Fn(&mut S, &mut LinkHandle<'a, K, M>) -> () {
        self.closer = Box::new(close_f);
        self
    }

    pub fn internal_handler<H, J>(mut self, handler: H) -> Self
    where
        H: Into<(K, J)>,
        J: for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send + 'static,
    {
        let (id, handler) = handler.into();
        self.internal_handlers.insert(id, Box::new(handler));
        self
    }

    pub fn external_handler<H, J>(mut self, handler: H) -> Self
    where
        H: Into<(K, J)>,
        J: for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send + 'static,
    {
        let (id, handler) = handler.into();
        self.external_handlers.insert(id.into(), Box::new(handler));
        self
    }
}

/// It is useful to be able to spawn a link when you have the bundled channels and ids
impl<S, K, M> Into<LinkSpawner<K, M>> for LinkParams<S, K, M>
where
    S: 'static + Send,
    M: 'static + Send,
    K: 'static + Eq + Hash + Send,
{
    fn into(self) -> LinkSpawner<K, M> {
        Box::new(move |(source, target, source_id, target_id)| {
            let handles = LinkState {
                source,
                target,
                source_id,
                target_id,
            };

            Box::new(Link::new(handles, self))
        })
    }
}
