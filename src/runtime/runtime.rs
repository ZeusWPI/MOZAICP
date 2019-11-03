use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use capnp::traits::{HasTypeId, Owned};
use core_capnp::{drop, initialize, mozaic_message};

use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};
use tokio;

use tracing::{error, field, info, span, trace, Level};
use tracing_futures::Instrument;

use rand;
use rand::Rng;

use messaging::reactor::*;
use messaging::types::*;

use errors::ErrorKind::{MozaicError, NoSuchReactorError};
use errors::{self, Consumable, Result, ResultExt};

use HasNamedTypeId;

/// The main runtime
pub struct Runtime;

pub struct Broker {
    runtime_id: ReactorId,
    actors: HashMap<ReactorId, ActorData>,
}

impl Broker {
    pub fn new() -> Result<BrokerHandle> {
        let id: ReactorId = rand::thread_rng().gen();

        let broker = Broker {
            runtime_id: id.clone(),
            actors: HashMap::new(),
        };
        let broker = BrokerHandle {
            broker: Arc::new(Mutex::new(broker)),
        };

        return Ok(broker);
    }

    fn dispatch_message(&mut self, message: Message) -> Result<()> {
        let (receiver_id, sender_id): (ReactorId, ReactorId) = {
            let r = message.reader();
            let rr = r.get()?;

            (rr.get_receiver()?.into(), rr.get_sender()?.into())
        };

        let receiver = match self.actors.get_mut(&receiver_id.clone()) {
            Some(receiver) => receiver,
            None => {
                error!("No such reactor {:?}", receiver_id);
                return Err(errors::Error::from_kind(NoSuchReactorError(
                    receiver_id.clone(),
                )));
            }
        };

        receiver.tx.unbounded_send(message).map_err(move |_| {
            error!("Couldn't send {:?} -> {:?}", sender_id, receiver_id);
            format!("send failed {:?}", receiver_id)
        })?;
        Ok(())
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

    pub fn dispatch_message(&mut self, message: Message) -> Result<()> {
        let mut broker = self.broker.lock().unwrap();
        broker.dispatch_message(message)
    }

    pub fn register(&mut self, id: ReactorId, tx: mpsc::UnboundedSender<Message>) {
        info!("Registering reactor {:?}", id);

        let mut broker = self.broker.lock().unwrap();
        broker.actors.insert(id, ActorData { tx });
    }

    pub fn register_as(&mut self, id: ReactorId, same_as: ReactorId) {
        trace!("Registering new reactor as {:?}", id);

        let tx = {
            let broker = self.broker.lock().unwrap();
            broker.actors.get(&same_as).unwrap().tx.clone()
        };

        self.register(id, tx);
    }

    pub fn drop_reactor(&mut self, _id: &ReactorId) {}

    pub fn unregister(&mut self, id: &ReactorId) {
        // Maybe check to close all links to this reactor
        info!("Unregistering reactor {:?}", id);

        let mut broker = self.broker.lock().unwrap();
        broker.actors.remove(&id);
    }

    pub fn send_message_self<M, F>(
        &mut self,
        target: &ReactorId,
        m: M,
        initializer: F,
    ) -> Result<()>
    where
        F: for<'b> FnOnce(capnp::any_pointer::Builder<'b>),
        M: Owned<'static>,
        <M as Owned<'static>>::Builder: HasNamedTypeId,
    {
        trace!("Sending message as runtime to {:?}", target);

        self.send_message(&self.get_runtime_id(), target, m, initializer)
    }

    pub fn send_message<M, F>(
        &mut self,
        sender: &ReactorId,
        target: &ReactorId,
        _m: M,
        initializer: F,
    ) -> Result<()>
    where
        F: for<'b> FnOnce(capnp::any_pointer::Builder<'b>),
        M: Owned<'static>,
        <M as Owned<'static>>::Builder: HasNamedTypeId,
    {
        trace!(
            "Sending {} {} message {:?} -> {:?}",
            <M as Owned<'static>>::Builder::type_id(),
            <M as Owned<'static>>::Builder::get_name(),
            sender,
            target
        );

        let mut broker = self.broker.lock().unwrap();

        if target == &broker.runtime_id {
            return Err(errors::Error::from_kind(MozaicError("This is not how you distribute a message locally. Target and runtime_id are the same...")));
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
        broker.dispatch_message(msg)
    }

    pub fn reactor_exists(&self, id: &ReactorId) -> bool {
        self.broker.lock().unwrap().actors.contains_key(id)
    }

    pub fn spawn<S>(
        &mut self,
        id: ReactorId,
        core_params: CoreParams<S, Runtime>,
        name: &str,
    ) -> Result<()>
    where
        S: 'static + Send,
    {
        info!("Spawning new reactor {} {:?}", name, id);

        let mut driver = {
            let mut broker = self.broker.lock().unwrap();

            let reactor = Reactor {
                id: id.clone(),
                internal_state: core_params.state,
                internal_handlers: core_params.handlers,
                links: HashMap::new(),
            };

            let (tx, rx) = mpsc::unbounded();
            broker.actors.insert(id.clone(), ActorData { tx });

            ReactorDriver {
                broker: self.clone(),
                internal_queue: VecDeque::new(),
                message_chan: rx,
                reactor,
                should_close: false,
                external_msgs_handled: 0,
                internal_msgs_handled: 0,
            }
        };

        {
            let mut ctx_handle = DriverHandle {
                broker: &mut driver.broker,
                internal_queue: &mut driver.internal_queue,
            };

            let mut reactor_handle = driver.reactor.handle(&mut ctx_handle);

            let initialize = MsgBuffer::<initialize::Owned>::new();
            reactor_handle.send_internal(initialize)?;
        }

        tokio::spawn(driver.instrument(span!(
            Level::TRACE,
            "driver",
            name = name,
            id = field::debug(&id)
        )));

        Ok(())
    }
}

enum InternalOp {
    Message(Message),
    OpenLink(Box<dyn LinkParamsTrait<Runtime>>),
    CloseLink(ReactorId),
    Destroy(),
}

pub struct ReactorDriver<S: 'static> {
    message_chan: mpsc::UnboundedReceiver<Message>,
    internal_queue: VecDeque<InternalOp>,
    broker: BrokerHandle,

    reactor: Reactor<S, Runtime>,
    should_close: bool,
    internal_msgs_handled: usize,
    external_msgs_handled: usize,
}

impl<S: 'static> ReactorDriver<S> {
    fn handle_external_message(&mut self, message: Message) {
        let span = span!(Level::INFO, "e_msg", c = self.external_msgs_handled);
        let _guard = span.enter();

        self.external_msgs_handled += 1;

        let mut handle = DriverHandle {
            internal_queue: &mut self.internal_queue,
            broker: &mut self.broker,
        };
        self.reactor
            .handle_external_message(&mut handle, message)
            .chain_err(|| "handling failed")
            .display();
    }

    fn handle_internal_queue(&mut self) {
        while let Some(op) = self.internal_queue.pop_front() {
            let span = span!(Level::INFO, "i_msg", c = self.internal_msgs_handled);
            self.internal_msgs_handled += 1;
            let _guard = span.enter();
            match op {
                InternalOp::Message(msg) => {
                    let mut handle = DriverHandle {
                        internal_queue: &mut self.internal_queue,
                        broker: &mut self.broker,
                    };
                    self.reactor
                        .handle_internal_message(&mut handle, msg)
                        .chain_err(|| "handling failed")
                        .display();
                }
                InternalOp::Destroy() => {
                    self.reactor.destroy().expect("I failed");
                }
                InternalOp::OpenLink(params) => {
                    let uuid = params.remote_id().clone();
                    info!(
                        "Open link {:?} -> {:?}",
                        field::debug(&self.reactor.id),
                        field::debug(&uuid)
                    );

                    let link = params.into_link();
                    let span = span!(tracing::Level::INFO, "link");

                    self.reactor.links.insert(uuid, (link, span));
                }
                InternalOp::CloseLink(uuid) => {
                    info!(
                        "Close link {:?} -> {:?}",
                        field::debug(&self.reactor.id),
                        field::debug(&uuid)
                    );

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

            if self.should_close {
                // if self.reactor.links.keys().all(|k| k == &self.reactor.id) {
                // all internal ops have been handled and no new messages can
                // arrive, so the reactor can be terminated.
                self.broker.unregister(&self.reactor.id);

                return Ok(Async::Ready(()));
            }

            // close if you have no links, you should close 'auto' links yourself.
            if self.reactor.links.is_empty() {
                {
                    let mut ctx_handle = DriverHandle {
                        broker: &mut self.broker,
                        internal_queue: &mut self.internal_queue,
                    };

                    let mut reactor_handle = self.reactor.handle(&mut ctx_handle);

                    let drop = MsgBuffer::<drop::Owned>::new();
                    reactor_handle
                        .send_internal(drop)
                        .expect("Failed to send drop message");
                }

                self.should_close = true;
            }

            match try_ready!(self.message_chan.poll()) {
                None => return Ok(Async::Ready(())),
                Some(item) => {
                    self.handle_external_message(item);
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
    fn dispatch_internal(&mut self, msg: Message) -> Result<()> {
        self.internal_queue.push_back(InternalOp::Message(msg));
        Ok(())
    }

    fn dispatch_external(&mut self, msg: Message) -> Result<()> {
        self.broker.dispatch_message(msg)
    }

    fn spawn<T>(&mut self, params: CoreParams<T, Runtime>, name: &str) -> Result<ReactorId>
    where
        T: 'static + Send,
    {
        let id: ReactorId = rand::thread_rng().gen();
        self.broker.spawn(id.clone(), params, name)?;
        Ok(id)
    }

    fn open_link<T>(&mut self, params: LinkParams<T, Runtime>) -> Result<()>
    where
        T: 'static + Send,
    {
        self.internal_queue
            .push_back(InternalOp::OpenLink(Box::new(params)));
        Ok(())
    }

    fn close_link(&mut self, id: &ReactorId) -> Result<()> {
        self.internal_queue
            .push_back(InternalOp::CloseLink(id.clone()));
        Ok(())
    }
}
