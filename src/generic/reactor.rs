
use super::*;

///
/// Reactor is the meat and the potatoes of MOZAIC
/// You can register Handers (just functions)
/// That are called when the correct data T is being handled
///
pub struct Reactor<S, K, M>
where
    K: Hash + Eq,
{
    id: ReactorID,
    state: S,
    msg_handlers: HashMap<K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>>,

    tx: Sender<K, M>,
    rx: Receiver<K, M>,
}

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
{
    pub fn new(state: S, id: ReactorID) -> Self {
        let (tx, rx) = mpsc::unbounded();

        Reactor {
            id,
            state,

            msg_handlers: HashMap::new(),
            tx,
            rx,
        }
    }

    pub fn add_handler<H>(&mut self, handler: H)
    where
        H: Into<(K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.msg_handlers.insert(id, handler);
    }

    pub fn get_handle(&self) -> ReactorHandle<K, M> {
        ReactorHandle {
            chan: self.tx.clone(),
        }
    }
}

/// Reactors get spawned with tokio, they only read from their channel and act on the messages
/// They reduce over an OperationStream
impl<S, K, M> Future for Reactor<S, K, M>
where
    K: Hash + Eq,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.rx.poll()) {
                None => return Ok(Async::Ready(())),
                Some(item) => match item {
                    Operation::InternalMessage(id, mut msg) => {
                        let mut handle = self.get_handle();

                        let ctx = Context {
                            state: &mut self.state,
                            handle: &mut handle,
                        };

                        self.msg_handlers
                            .get_mut(&id)
                            .map(|h| h.handle(ctx, &mut msg));
                    },
                    Operation::Close() => return Ok(Async::Ready(())),

                    _ => {
                        unimplemented!();
                    },
                },
            }
        }
    }
}

///
/// ReactorHandle wraps a channel to send operations to the reactor
///
pub struct ReactorHandle<K, M> {
    chan: Sender<K, M>,
}

/// A context with a ReactorHandle is a ReactorContext
type ReactorContext<'a, S, K, M> = Context<'a, S, ReactorHandle<K, M>>;

// ANCHOR Implementation with any::TypeId
/// To use MOZAIC a few things
/// Everything your handlers use, so the Context and how to get from M to T
/// This is already implemented for every T, with the use of Rusts TypeIds
/// But you may want to implement this again, with for example Capnproto messages
/// so you can send messages over the internet
impl ReactorHandle<any::TypeId, Message> {
    pub fn open_link(&mut self) {
        unimplemented!();
    }

    pub fn send_internal<T: 'static>(&mut self, msg: T) {
        let id = any::TypeId::of::<T>();
        let msg = Message::from(msg);
        self.chan
            .unbounded_send(Operation::InternalMessage(id, msg))
            .expect("crashed");
    }

    pub fn close(&mut self) {
        self.chan
            .unbounded_send(Operation::Close())
            .expect("Couldn't close");
    }

    pub fn spawn(&mut self, _params: u32) {
        unimplemented!();
    }
}
