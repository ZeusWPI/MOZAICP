use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use core_capnp::{mozaic_message, initialize};
use capnp::traits::{Owned, HasTypeId};

use tokio;
use futures::{Future, Async, Poll, Stream};
use futures::sync::mpsc;

use rand;
use rand::Rng;

use messaging::types::*;
use messaging::reactor::*;


/// The main runtime
pub struct Runtime;

pub struct Broker {
    runtime_id: ReactorId,
    actors: HashMap<ReactorId, ActorData>,
}

impl Broker {
    pub fn new() -> BrokerHandle {
        let id: ReactorId = rand::thread_rng().gen();

        let broker = Broker {
            runtime_id: id,
            actors: HashMap::new(),
        };
        return BrokerHandle { broker: Arc::new(Mutex::new(broker)) };
    }

    fn dispatch_message(&mut self, message: Message) {
        let receiver_id = message.reader()
            .get()
            .unwrap()
            .get_receiver()
            .unwrap()
            .into();

        if let Some(receiver) = self.actors.get_mut(&receiver_id) {
            receiver.tx.unbounded_send(message)
                .expect("send failed");
        } else {
            panic!("no such actor: {:?}", receiver_id);
        }
    }
}

pub struct ActorData {
    tx: mpsc::UnboundedSender<Message>,
}

#[derive(Clone)]
pub struct BrokerHandle {
    broker: Arc<Mutex<Broker>>,
}

impl BrokerHandle {
    pub fn get_runtime_id(&self) -> ReactorId {
        self.broker.lock().unwrap().runtime_id.clone()
    }

    pub fn dispatch_message(&mut self, message: Message) {
        let mut broker = self.broker.lock().unwrap();
        broker.dispatch_message(message);
    }

    pub fn register(&mut self, id: ReactorId, tx: mpsc::UnboundedSender<Message>) {
        let mut broker = self.broker.lock().unwrap();
        broker.actors.insert(id, ActorData { tx });
    }

    pub fn unregister(&mut self, id: &ReactorId) {
        let mut broker = self.broker.lock().unwrap();
        broker.actors.remove(&id);
    }


    pub fn send_message_self<M, F>(&mut self, target: &ReactorId, m: M, initializer: F)
        where F: for<'b> FnOnce(capnp::any_pointer::Builder<'b>),
              M: Owned<'static>,
              <M as Owned<'static>>::Builder: HasTypeId,
    {
        self.send_message(&self.get_runtime_id(), target, m, initializer)
    }

    pub fn send_message<M, F>(&mut self, sender: &ReactorId, target: &ReactorId, _m: M, initializer: F)
        where F: for<'b> FnOnce(capnp::any_pointer::Builder<'b>),
              M: Owned<'static>,
              <M as Owned<'static>>::Builder: HasTypeId,

    {
        let mut broker = self.broker.lock().unwrap();

        if target == &broker.runtime_id {
            panic!("This is not how you distribute a message locally. Target and runtime_id are the same...")
        }

        let mut message_builder = ::capnp::message::Builder::new_default();
        {
            let mut msg = message_builder.init_root::<mozaic_message::Builder>();

            msg.set_sender(sender.bytes());
            msg.set_receiver(target.bytes());

            msg.set_type_id(<M as Owned<'static>>::Builder::type_id());
            {
                let payload_builder = msg.reborrow().init_payload();
                initializer(payload_builder);
            }
        }

        let msg = Message::from_capnp(message_builder.into_reader());
        broker.dispatch_message(msg);
    }

    pub fn spawn<S>(&mut self, id: ReactorId, core_params: CoreParams<S, Runtime>)
        where S: 'static + Send
    {
        let mut broker = self.broker.lock().unwrap();

        let reactor = Reactor {
            id: id.clone(),
            internal_state: core_params.state,
            internal_handlers: core_params.handlers,
            links: HashMap::new(),
        };

        let (tx, rx) = mpsc::unbounded();

        broker.actors.insert(id.clone(), ActorData { tx });

        let mut driver = ReactorDriver {
            broker: self.clone(),
            internal_queue: VecDeque::new(),
            message_chan: rx,
            reactor,
        };
        {
            let mut ctx_handle = DriverHandle {
                broker: &mut driver.broker,
                internal_queue: &mut driver.internal_queue,
            };

            let mut reactor_handle = driver.reactor.handle(&mut ctx_handle);
            let initialize = MsgBuffer::<initialize::Owned>::new();
            reactor_handle.send_internal(initialize);
        }

        tokio::spawn(driver);
    }
}

enum InternalOp {
    Message(Message),
    OpenLink(Box<dyn LinkParamsTrait<Runtime>>),
    CloseLink(ReactorId),
}


pub struct ReactorDriver<S: 'static> {
    message_chan: mpsc::UnboundedReceiver<Message>,
    internal_queue: VecDeque<InternalOp>,
    broker: BrokerHandle,

    reactor: Reactor<S, Runtime>,
}

impl<S: 'static> ReactorDriver<S> {
    fn handle_external_message(&mut self, message: Message) {
        let mut handle = DriverHandle {
            internal_queue: &mut self.internal_queue,
            broker: &mut self.broker,
        };
        self.reactor.handle_external_message(&mut handle, message)
            .expect("handling failed");
    }

    fn handle_internal_queue(&mut self) {
        while let Some(op) = self.internal_queue.pop_front() {
            match op {
                InternalOp::Message(msg) => {
                    let mut handle = DriverHandle {
                        internal_queue: &mut self.internal_queue,
                        broker: &mut self.broker,
                    };
                    self.reactor.handle_internal_message(&mut handle, msg)
                        .expect("handling failed");
                }
                InternalOp::OpenLink(params) => {
                    let uuid = params.remote_id().clone();
                    let link = params.into_link();
                    self.reactor.links.insert(uuid, link);
                }
                InternalOp::CloseLink(uuid) => {
                    self.reactor.links.remove(&uuid);
                }
            }
        }
    }
}

impl<S: 'static> Future for ReactorDriver<S> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            self.handle_internal_queue();

            if self.reactor.links.is_empty() {
                // all internal ops have been handled and no new messages can
                // arrive, so the reactor can be terminated.
                self.broker.unregister(&self.reactor.id);
                return Ok(Async::Ready(()));
            }

            match try_ready!(self.message_chan.poll()) {
                None => return Ok(Async::Ready(())),
                Some(message) => {
                    self.handle_external_message(message);
                }
            }
        }
    }
}

impl<'a> Context<'a> for Runtime {
    type Handle = DriverHandle<'a>;
}

pub struct DriverHandle<'a> {
    internal_queue: &'a mut VecDeque<InternalOp>,
    broker: &'a mut BrokerHandle,
}

impl<'a> CtxHandle<Runtime> for DriverHandle<'a> {

    fn dispatch_internal(&mut self, msg: Message) {
        self.internal_queue.push_back(InternalOp::Message(msg));
    }

    fn dispatch_external(&mut self, msg: Message) {
        self.broker.dispatch_message(msg);
    }

    fn spawn<T>(&mut self, params: CoreParams<T, Runtime>) -> ReactorId
        where T: 'static + Send
    {
        let id: ReactorId = rand::thread_rng().gen();
        self.broker.spawn(id.clone(), params);
        return id;
    }

    fn open_link<T>(&mut self, params: LinkParams<T, Runtime>)
        where T: 'static + Send
    {
        self.internal_queue.push_back(InternalOp::OpenLink(Box::new(params)));
    }

    fn close_link(&mut self, id: &ReactorId) {
        self.internal_queue.push_back(InternalOp::CloseLink(id.clone()));
    }
}
