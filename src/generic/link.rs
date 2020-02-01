use super::*;
use std::hash::Hash;

/// Macro to create link handles
/// This does not borrow the entire Link like a function would
macro_rules! linkHandle {
    ($e:expr) => {
        LinkHandle {
            state: &$e.link_state,
        };
    };
}

/// Package channels and id's neatly together
struct LinkState<K, M> {
    source: Sender<K, M>,
    target: Sender<K, M>,
    source_id: ReactorID,
    target_id: ReactorID,
}

// ANCHOR Link
/// A link pair links 2 reactors together
/// Which enables them to communicate with messages
pub struct Link<S, K, M> {
    state: S,

    internal_handlers: HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,
    external_handlers: HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,

    link_state: LinkState<K, M>,
}

impl<S, K, M> Link<S, K, M> {
    fn new(link_state: LinkState<K, M>, params: LinkParams<S, K, M>) -> Self {
        Self {
            link_state,
            state: params.state,
            internal_handlers: params.internal_handlers,
            external_handlers: params.external_handlers,
        }
    }
}

/// Handle to manipulate a link
/// Being able so send new messages and close the link
#[derive(Clone)]
pub struct LinkHandle<'a, K, M> {
    state: &'a LinkState<K, M>,
}

impl<'a, K, M> LinkHandle<'a, K, M>
where
{
    pub fn send_message<T: 'static + IntoMessage<K, M>>(&mut self, msg: T) {
        if let Some((id, msg)) = T::into_msg(msg) {
            self.state
                .target
                .unbounded_send(Operation::ExternalMessage(
                    self.state.source_id.clone(),
                    id,
                    msg,
                ))
                .expect("Link handle crashed");
        }
    }

    pub fn send_internal<T: 'static + IntoMessage<K, M>>(&mut self, msg: T) {
        if let Some((id, msg)) = T::into_msg(msg) {
            self.state
                .source
                .unbounded_send(Operation::InternalMessage(id, msg, false))
                .expect("Link handle crashed");
        }
    }

    pub fn close_link(&mut self) {
        self.state
            .source
            .unbounded_send(Operation::CloseLink(self.state.target_id))
            .expect("Link handle crashed");
    }

    pub fn target_id(&'a self) -> &'a ReactorID {
        &self.state.target_id
    }

    pub fn source_id(&'a self) -> &'a ReactorID {
        &self.state.source_id
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
                }
            }
            LinkOperation::ExternalMessage(id, message) => {
                if let Some(h) = self.external_handlers.get_mut(id) {
                    h.handle(&mut self.state, &mut linkHandle!(self), (id, message));
                }
            }
            LinkOperation::Close() => {
                if let Result::Err(_) = self
                    .link_state
                    .target
                    .unbounded_send(Operation::CloseLink(self.link_state.source_id))
                {
                    // The problem is this doesn't happen always
                    println!("Cannot send close message, channel closed");
                }
            }
        };
    }
}

// ANCHOR Params
/// Builder pattern for constructing links
pub struct LinkParams<S, K, M> {
    state: S,
    internal_handlers: HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,
    external_handlers: HashMap<K, Box<dyn for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send>>,
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

    pub fn internal_handler<H, J>(&mut self, handler: H)
    where
        H: Into<(K, J)>,
        J: for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send + 'static,
    {
        let (id, handler) = handler.into();
        self.internal_handlers.insert(id, Box::new(handler));
    }

    pub fn external_handler<H, J>(&mut self, handler: H)
    where
        H: Into<(K, J)>,
        J: for<'a> Handler<S, LinkHandle<'a, K, M>, (&'a K, &'a mut M)> + Send + 'static,
    {
        let (id, handler) = handler.into();
        self.external_handlers.insert(id.into(), Box::new(handler));
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
