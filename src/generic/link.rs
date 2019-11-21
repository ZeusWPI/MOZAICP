use super::*;
use std::hash::Hash;

// ANCHOR Link
pub struct Link<S, K, M> {
    state: S,

    internal_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
    external_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,

    handles: LinkHandle<K, M>,
}

impl<S, K, M> Link<S, K, M> {
    pub fn new(handles: LinkHandle<K, M>, params: LinkParams<S, K, M>) -> Self {
        Self {
            handles,
            state: params.state,
            internal_handlers: params.internal_handlers,
            external_handlers: params.external_handlers,
        }
    }
}

#[derive(Clone)]
pub struct LinkHandle<K, M> {
    source: Sender<K, M>,
    target: Sender<K, M>,
}

type LinkContext<'a, S, K, M> = Context<'a, S, LinkHandle<K, M>>;

impl<S, K, M> Handler<(), ReactorHandle<K, M>, LinkOperation<K, M>> for Link<S, K, M>
where
    K: Hash + Eq,
{
    fn handle(&mut self, _: Context<(), ReactorHandle<K, M>>, m: &mut LinkOperation<K, M>) {
        let ctx = LinkContext {
            state: &mut self.state,
            handle: &mut self.handles,
        };

        match m {
            LinkOperation::InternalMessage(id, message) => {
                self.internal_handlers.get_mut(id).map(|h| h.handle(ctx, message));
            },
            LinkOperation::ExternalMessage(id, message) => {
                self.external_handlers.get_mut(id).map(|h| h.handle(ctx, message));
            },
        };
    }
}

// ANCHOR Params
pub struct LinkParams<S, K, M> {
    state: S,
    internal_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
    external_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
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
        }
    }

    pub fn internal_handler<H>(&mut self, handler: H)
    where
        H: Into<(K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.internal_handlers.insert(id, handler);
    }

    pub fn external_handler<H>(&mut self, handler: H)
    where
        H: Into<(K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.external_handlers.insert(id, handler);
    }
}
