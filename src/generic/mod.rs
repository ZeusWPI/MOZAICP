
use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::marker::PhantomData;

mod message;
pub use self::message::Message;
mod types;
// ! Just some types to make things organised
pub use self::types::{TypeID, ReactorID};

// ANCHOR Traits

// ! This trait defines the types that can be transmitted over MOZAIC
pub trait IDed {
    fn get_id() -> TypeID;
}

pub trait Handler<S, C, M> {
    fn handle(&mut self, c: Context<S, C>, m: M);
}

pub struct Context<'a, S, C> {
    state: &'a mut S,
    handle: &'a mut C,
}


// ! The actual messages that are sent
enum Operation<M> {
    InternalMessage(TypeID, M),
    ExternalMessage(ReactorID, TypeID, M),
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
            broker: Arc::new(Mutex::new(broker))
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
    i_msg_h: HashMap<TypeID, Box<dyn Handler<S, ReactorHandle<M>, M>>>,

    self_chan: mpsc::UnboundedSender<Operation<M>>,
    msg_chan: mpsc::UnboundedReceiver<Operation<M>>,
}

type ReactorContext<'a, S, M> = Context<'a, S, ReactorHandle<M>>;

impl<S, M> Reactor<S, M> {
    pub fn add_handler(&mut self, id: TypeID, handler: Box<dyn Handler<S, ReactorHandle<M>, M>>) {
        self.i_msg_h.insert(id, handler);
    }

    fn get_handle(&self) -> ReactorHandle<M> {
        ReactorHandle {
            chan: self.self_chan.clone()
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
                Some(item) => {
                    match item {
                        Operation::InternalMessage(id, msg) => {
                            let mut handle = self.get_handle();

                            let ctx = Context {
                                state: &mut self.state,
                                handle: &mut handle,
                            };

                            self.i_msg_h.get_mut(&id).map(|h|
                                h.handle(ctx, msg)
                            );

                        },
                        Operation::ExternalMessage(_reactor, _id, _msg) => {
                            unimplemented!();
                        }
                    }
                }
            }
        }
    }
}

pub struct ReactorHandle<M> {
    chan: mpsc::UnboundedSender<Operation<M>>,
}

impl<M> ReactorHandle<M> {
    fn open_link(&mut self) {
        unimplemented!();
    }

    fn send_internal<T>(&mut self, _msg: T) {
        unimplemented!();
    }

    fn spawn(&mut self, _params: u32) {
        unimplemented!();
    }
}

pub struct ReactorHandler<T, F> {
    t: PhantomData<T>,
    f: F,
}

impl<S, M, T, F> Handler<S, ReactorHandle<M>, M> for ReactorHandler<T, F> {
    fn handle(&mut self, _c: ReactorContext<S, M>, _m: M) {
        unimplemented!();
    }
}
