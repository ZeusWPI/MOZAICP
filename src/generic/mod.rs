use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::any;

mod message;
pub use self::message::Message;
mod types;
// ! Just some types to make things organised
pub use self::types::{ReactorID};

// ANCHOR Traits

pub trait Handler<S, C, M> {
    fn handle(&mut self, c: Context<S, C>, m: &mut M);
}

pub struct Context<'a, S, C> {
    state: &'a mut S,
    handle: &'a mut C,
}

impl<'a, S, C> Context<'a, S, C> {
    fn split(self) -> (&'a mut S, &'a mut C) {
        (self.state, self.handle)
    }
}

// ! The actual messages that are sent
enum Operation<K, M> {
    InternalMessage(K, M),
    ExternalMessage(ReactorID, K, M),
    Close,
    // OpenLink
    // CloseLink
}

// ANCHOR Broker

struct Broker<K, M> {
    reactors: HashMap<ReactorID, mpsc::UnboundedSender<Operation<K, M>>>,
}

#[derive(Clone)]
pub struct BrokerHandle<K, M> {
    broker: Arc<Mutex<Broker<K, M>>>,
}

impl<K, M> BrokerHandle<K, M> {
    pub fn new() -> BrokerHandle<K, M> {
        let broker = Broker {
            reactors: HashMap::new(),
        };

        BrokerHandle {
            broker: Arc::new(Mutex::new(broker)),
        }
    }

    pub fn spawn(&self, _params: u32) -> ReactorID {
        unimplemented!()
    }

    pub fn remove(&self, params: &ReactorID) {
        let mut broker = self.broker.lock().unwrap();
        broker.reactors.remove(&params);
    }
}

// ANCHOR Reactor

pub struct Reactor<S, K, M>
where
    K: Hash + Eq,
{
    id: ReactorID,
    state: S,
    i_msg_h: HashMap<K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>>,

    tx: mpsc::UnboundedSender<Operation<K, M>>,
    rx: mpsc::UnboundedReceiver<Operation<K, M>>,
}

type ReactorContext<'a, S, K, M> = Context<'a, S, ReactorHandle<K, M>>;

impl<S, K, M> Reactor<S, K, M>
where
    K: Hash + Eq,
{
    pub fn new(state: S, id: ReactorID) -> Self {
        let (tx, rx) = mpsc::unbounded();

        Reactor {
            id,
            state,

            i_msg_h: HashMap::new(),
            tx,
            rx,
        }
    }

    pub fn add_handler<H>(&mut self, handler: H)
    where
        H: Into<(K, Box<dyn Handler<S, ReactorHandle<K, M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.i_msg_h.insert(id, handler);
    }

    pub fn get_handle(&self) -> ReactorHandle<K, M> {
        ReactorHandle {
            chan: self.tx.clone(),
        }
    }
}

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

                        self.i_msg_h.get_mut(&id).map(|h| h.handle(ctx, &mut msg));
                    }

                    Operation::ExternalMessage(_reactor, _id, _msg) => {
                        unimplemented!();
                    }

                    Operation::Close => return Ok(Async::Ready(())),
                },
            }
        }
    }
}

pub struct ReactorHandle<K, M> {
    chan: mpsc::UnboundedSender<Operation<K, M>>,
}

impl ReactorHandle<any::TypeId, Message> {
    pub fn open_link(&mut self) {
        unimplemented!();
    }

    pub fn send_internal<T:'static>(&mut self, msg: T) {
        let id = any::TypeId::of::<T>();
        let msg = Message::from(msg);
        self.chan
            .unbounded_send(Operation::InternalMessage(id, msg))
            .expect("crashed");
    }

    pub fn close(&mut self) {
        self.chan
            .unbounded_send(Operation::Close)
            .expect("Couldn't close");
    }

    pub fn spawn(&mut self, _params: u32) {
        unimplemented!();
    }
}

pub struct ReactorHandler<S, T, F> {
    t: PhantomData<T>,
    s: PhantomData<S>,
    f: F,
}

impl<S, T, F> ReactorHandler<S, T, F>
where
    F: Fn(&mut S, &mut ReactorHandle<any::TypeId, Message>, &T) -> () + Send,
    T: 'static,
{
    pub fn from(f: F) -> Box<Self> {
        Box::new(Self {
            t: PhantomData,
            s: PhantomData,
            f,
        })
    }
}

impl<S, F, T>
    Into<(
        any::TypeId,
        Box<dyn Handler<S, ReactorHandle<any::TypeId, Message>, Message> + Send>,
    )> for Box<ReactorHandler<S, T, F>>
where
    F: 'static + Send + Fn(&mut S, &mut ReactorHandle<any::TypeId, Message>, &T) -> (),
    T: 'static + Send,
    S: 'static + Send,
{
    fn into(
        self,
    ) -> (
        any::TypeId,
        Box<dyn Handler<S, ReactorHandle<any::TypeId, Message>, Message> + Send>,
    ) {
        let id = any::TypeId::of::<T>();
        (id, self)
    }
}

impl<S, T, F> Handler<S, ReactorHandle<any::TypeId, Message>, Message> for ReactorHandler<S, T, F>
where
    F: Fn(&mut S, &mut ReactorHandle<any::TypeId, Message>, &T) -> () + Send,
    T: 'static,
{
    fn handle(&mut self, c: ReactorContext<S, any::TypeId, Message>, m: &mut Message) {
        let (state, handle) = c.split();
        m.borrow()
            .map(|t| (self.f)(state, handle, t))
            .expect("fuck off");
    }
}
