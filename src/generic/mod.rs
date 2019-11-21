use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

mod message;
pub use self::message::Message;
mod types;
// ! Just some types to make things organised
pub use self::types::{ReactorID, TypeID};

// ANCHOR Traits

// ! This trait defines the types that can be transmitted over MOZAIC
pub trait IDed {
    fn get_id() -> TypeID;
}

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
enum Operation<M> {
    InternalMessage(TypeID, M),
    ExternalMessage(ReactorID, TypeID, M),
    Close,
    // OpenLink
    // CloseLink
}

// ANCHOR Broker

struct Broker<M> {
    reactors: HashMap<ReactorID, mpsc::UnboundedSender<Operation<M>>>,
}

#[derive(Clone)]
pub struct BrokerHandle<M> {
    broker: Arc<Mutex<Broker<M>>>,
}

impl<M> BrokerHandle<M> {
    pub fn new() -> BrokerHandle<M> {
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

pub struct Reactor<S, M> {
    id: ReactorID,
    state: S,
    i_msg_h: HashMap<TypeID, Box<dyn Handler<S, ReactorHandle<M>, M> + Send>>,

    self_chan: mpsc::UnboundedSender<Operation<M>>,
    msg_chan: mpsc::UnboundedReceiver<Operation<M>>,
}

type ReactorContext<'a, S, M> = Context<'a, S, ReactorHandle<M>>;

impl<S, M> Reactor<S, M> {
    pub fn new(state: S, id: ReactorID) -> Self {
        let (tx, rx) = mpsc::unbounded();

        Reactor {
            id,
            state,

            i_msg_h: HashMap::new(),
            self_chan: tx,
            msg_chan: rx,
        }
    }

    pub fn add_handler<H>(&mut self, handler: H)
    where
        H: Into<(TypeID, Box<dyn Handler<S, ReactorHandle<M>, M> + Send>)>,
    {
        let (id, handler) = handler.into();
        self.i_msg_h.insert(id, handler);
    }

    pub fn get_handle(&self) -> ReactorHandle<M> {
        ReactorHandle {
            chan: self.self_chan.clone(),
        }
    }
}

impl<S, M> Future for Reactor<S, M> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.msg_chan.poll()) {
                None => return Ok(Async::Ready(())),
                Some(item) => match item {
                    Operation::InternalMessage(id, mut msg) => {
                        let mut handle = self.get_handle();

                        let ctx = Context {
                            state: &mut self.state,
                            handle: &mut handle,
                        };

                        self.i_msg_h.get_mut(&id).map(|h| h.handle(ctx, &mut msg));
                    },
                    Operation::ExternalMessage(_reactor, _id, _msg) => {
                        unimplemented!();
                    },
                    Operation::Close => {
                        return Ok(Async::Ready(()))
                    },
                },
            }
        }
    }
}

pub struct ReactorHandle<M> {
    chan: mpsc::UnboundedSender<Operation<M>>,
}

impl ReactorHandle<Message> {
    pub fn open_link(&mut self) {
        unimplemented!();
    }

    pub fn send_internal<T: IDed + 'static>(&mut self, msg: T) {
        let id = T::get_id();
        let msg = Message::from(msg);
        self.chan
            .unbounded_send(Operation::InternalMessage(id, msg))
            .expect("crashed");
    }

    pub fn close(&mut self) {
        self.chan.unbounded_send(Operation::Close).expect("Couldn't close");
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
    F: Fn(&mut S, &mut ReactorHandle<Message>, &T) -> () + Send,
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

impl<S, F, T> Into<(TypeID, Box<dyn Handler<S, ReactorHandle<Message>, Message> + Send>)> for Box<ReactorHandler<S, T, F>>
where
    F: 'static + Send + Fn(&mut S, &mut ReactorHandle<Message>, &T) -> (),
    T: 'static + Send + IDed,
    S: 'static + Send,
{
    fn into(self) -> (TypeID, Box<dyn Handler<S, ReactorHandle<Message>, Message> + Send>) {
        let id = T::get_id();
        (id, self)
    }
}

impl<S, T, F> Handler<S, ReactorHandle<Message>, Message> for ReactorHandler<S, T, F>
where
    F: Fn(&mut S, &mut ReactorHandle<Message>, &T) -> () + Send,
    T: 'static,
{
    fn handle(&mut self, c: ReactorContext<S, Message>, m: &mut Message) {
        let (state, handle) = c.split();
        m.borrow()
            .map(|t| (self.f)(state, handle, t))
            .expect("fuck off");
    }
}
