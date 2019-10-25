extern crate mozaic;
extern crate hex;
extern crate tokio;
extern crate futures;
extern crate rand;
extern crate cursive;
extern crate capnp;

use tokio::prelude::Stream;
use tokio::sync::mpsc;
use futures::Future;

use mozaic::core_capnp::{initialize, terminate_stream, identify, actor_joined};
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::errors::*;
use mozaic::base_capnp::{client_message, host_message};
use mozaic::connection_capnp::{client_kicked};
use mozaic::runtime::{self, Broker, BrokerHandle};

use std::thread;
use std::env;

use rand::Rng;

use cursive::align::VAlign;
use cursive::Cursive;
use cursive::theme::Theme;
use cursive::traits::{Boxable, Identifiable};
use cursive::views::{TextView, EditView, LinearLayout};

fn main() {
    let args: Vec<String> = env::args().collect();
    let id = args.get(1).unwrap().parse().unwrap();

    let addr = "127.0.0.1:9142".parse().unwrap();
    let self_id: ReactorId = rand::thread_rng().gen();

    // Creates the cursive root - required for every application.
    let mut siv = Cursive::default();

    siv.set_theme({
        let mut t = Theme::default();
        t.shadow = false;
        t
    });

    // ugly chat view
    siv.add_layer(LinearLayout::vertical()
        .child(TextView::empty()
            .v_align(VAlign::Bottom)
            .with_id("messages")
            .full_height())
        .child(EditView::new()
            .with_id("input")
            .full_width())
    );

    let cb_sink = siv.cb_sink().clone();

    let (tx, rx) = mpsc::channel::<String>(10);

    let _cb_sink = cb_sink.clone();
    let recv = rx.for_each(move |message| {
        _cb_sink.send(Box::new(|cursive: &mut Cursive| {
            cursive.call_on_id("messages", |view: &mut TextView| {
                view.append(message);
                view.append("\n");
            });
        })).expect("Civ error");

        Ok(())
    }).map_err(|_| ());

    thread::spawn(move || {
        tokio::run(futures::lazy(move || {
                tokio::spawn(recv);
                let mut broker = Broker::new().unwrap();

                let reactor = ClientReactor {
                    server: None,
                    id,
                    broker: broker.clone(),
                    tx,
                };
                broker.spawn(self_id.clone(), reactor.params(), "main").display();

                let _broker = broker.clone();
                let _self_id = self_id.clone();

                cb_sink.send(Box::new(|cursive: &mut Cursive| {
                    cursive.call_on_id("input", |view: &mut EditView| {
                        view.set_on_submit(move |cursive, input_text| {

                            _broker.clone().send_message(
                                &_self_id, &_self_id,
                                client_message::Owned,
                                |b| {
                                    let mut input: client_message::Builder = b.init_as();
                                    input.set_data(input_text.as_bytes());
                                }).expect("here");

                            cursive.call_on_id("input", |view: &mut EditView| {
                                view.set_content("");
                            });
                        })
                    });
                })).expect("Civ error 2");

                tokio::spawn(runtime::connect_to_server(broker, self_id, &addr));

                Ok(())
        }));
    });

    siv.set_fps(10);
    // Starts the event loop.
    siv.run();
}

// Main client logic
/// ? greeter_id is the server, the tcp stream that you connected to
/// ? runtime_id is your own runtime, to handle visualisation etc
/// ? user are you  ...
struct ClientReactor {
    server: Option<ReactorId>,
    id: u64,
    broker: BrokerHandle,
    tx: mpsc::Sender<String>,
}

impl ClientReactor {
    fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::open_host));
        return params;
    }

    // reactor setup
    fn initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        let runtime_link = RuntimeLink::params(handle.id().clone());
        handle.open_link(runtime_link)?;

        return Ok(());
    }

    fn open_host<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()>
    {
        let id = r.get_id()?;

        if let Some(server) = &self.server {
            handle.open_link(HostLink::params(self.tx.clone(), ReactorId::from(id)))?;
            self.broker.register_as(id.into(), server.clone());

        } else {

            handle.open_link(ServerLink::params(id.into()))?;
            self.server = Some(id.into());

            let mut identify = MsgBuffer::<identify::Owned>::new();
            identify.build(|b| {
                b.set_key(self.id);
            });
            handle.send_internal(identify).display();
        }

        Ok(())
    }
}

// Handler for the connection with the chat server
struct ServerLink;

impl ServerLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, ServerLink);

        params.external_handler(
            terminate_stream::Owned,
            CtxHandler::new(Self::close_handler),
        );

        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(Self::handle_actor_joined),
        );

        params.internal_handler(
            identify::Owned,
            CtxHandler::new(Self::identify),
        );

        return params;
    }

    fn handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        id: actor_joined::Reader,
    ) -> Result<()> {
        let id = id.get_id()?;

        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id));
        handle.send_internal(joined)?;

        Ok(())
    }

    fn identify<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        id: identify::Reader,
    ) -> Result<()> {
        let id = id.get_key();

        let mut chat_message = MsgBuffer::<identify::Owned>::new();
        chat_message.build(|b| {
            b.set_key(id);
        });

        handle.send_message(chat_message).display();
        Ok(())
    }

    fn close_handler<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: terminate_stream::Reader,
    ) -> Result<()>
    {
        // also close our end of the stream
        handle.close_link()?;
        return Ok(());
    }
}

struct HostLink {
    tx: mpsc::Sender<String>,
}
impl HostLink {
    fn params<C: Ctx>(tx: mpsc::Sender<String>, remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, HostLink { tx });

        params.external_handler(
            host_message::Owned,
            CtxHandler::new(Self::receive_chat_message),
        );

        params.internal_handler(
            client_message::Owned,
            CtxHandler::new(client_message::i_to_e),
        );

        params.external_handler(
            client_kicked::Owned,
            CtxHandler::new(Self::client_kicked),
        );

        return params;
    }

    // pick up a 'send_message' event from the reactor, and put it to effect
    // by constructing the chat message and sending it to the chat server.
    fn client_kicked<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: client_kicked::Reader,
    ) -> Result<()>
    {
        let mut chat_message = MsgBuffer::<host_message::Owned>::new();
        chat_message.build(|b| {
            b.set_data(b"You got kicked");
        });
        handle.send_internal(chat_message)?;

        let chat_message = MsgBuffer::<client_kicked::Owned>::new();
        handle.send_internal(chat_message)?;

        handle.close_link()?;

        return Ok(());
    }

    // receive a chat message from the chat server, and broadcast it on the
    // reactor.
    fn receive_chat_message<C: Ctx>(
        &mut self,
        _handle: &mut LinkHandle<C>,
        host_message: host_message::Reader,
    ) -> Result<()>
    {
        let message = host_message.get_data()?;
        self.tx.try_send(String::from_utf8_lossy(&message).to_string()).expect("Fuck off");

        return Ok(());
    }
}

struct RuntimeLink;

impl RuntimeLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);

        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(actor_joined::e_to_i),
        );

        params.external_handler(client_message::Owned, CtxHandler::new(Self::receive_chat_message));

        return params;
    }

    // receive a chat message from the chat server, and broadcast it on the
    // reactor.
    fn receive_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        client_message: client_message::Reader,
    ) -> Result<()>
    {
        let message = client_message.get_data()?;

        let mut chat_message = MsgBuffer::<client_message::Owned>::new();
        chat_message.build(|b| {
            b.set_data(message);
        });

        handle.send_internal(chat_message)?;

        return Ok(());
    }
}
