use super::*;

// ANCHOR Link
pub struct Link<S, K, M> {
    state: S,

    internal_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
    external_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,

    source: Sender<K, M>,
    target: Sender<K, M>,
}

pub struct LinkHandle<K, M> {
    source: Sender<K, M>,
    target: Sender<K, M>,
}

type LinkContext<'a, S, K, M> = Context<'a, S, LinkHandle<K, M>>;
