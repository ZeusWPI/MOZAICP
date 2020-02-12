
use crate::generic::{Handler};
use crate::generic::reactor::ReactorHandle;

use std::collections::HashMap;
use std::hash::Hash;


type HandlersMap<S, K, M> = HashMap<K, Box<dyn for<'a> Handler<S, ReactorHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>;
/// Builder pattern for constructing reactors
pub struct CoreParams<S, K, M> {
    state: S,
    handlers: HandlersMap<S, K, M>,
}

impl<S, K, M> CoreParams<S, K, M> {
    pub fn consume(self) -> (S, HandlersMap<S, K, M>,) {
        (
            self.state, self.handlers,
        )
    }
}

impl<S, K, M> CoreParams<S, K, M>
where
    K: Eq + Hash + 'static,
    M: 'static,
{
    pub fn new(state: S) -> Self {
        CoreParams {
            state,
            handlers: HashMap::new(),
        }
    }

    pub fn handler<H, J>(mut self, handler: H) -> Self
    where
        H: Into<(K, J)>,
        J: for<'a> Handler<S, ReactorHandle<'a, K, M>, (&'a K, &'a mut M)> + Send + 'static,
    {
        let (id, handler) = handler.into();
        self.handlers.insert(id, Box::new(handler));
        self
    }
}
