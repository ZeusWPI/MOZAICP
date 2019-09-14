extern crate mozaic;
extern crate hex;
extern crate tokio;
#[macro_use]
extern crate futures;
extern crate rand;
extern crate cursive;
extern crate capnp;

use tokio::net::TcpStream;
use futures::sync::mpsc;
use futures::Future;

use mozaic::core_capnp::{initialize, terminate_stream, identify, actor_joined};
use mozaic::chat_capnp;
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::client::{LinkHandler, RuntimeState};
use mozaic::errors::*;
use mozaic::client_capnp::{client_message, host_message, client_kicked};
use mozaic::server::runtime::{Broker};
use mozaic::server;

use rand::Rng;

use std::thread;
use std::env;
use std::sync::{Arc, Mutex};
use std::str;

// pub mod chat {
//     include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
// }

fn main() {
    let args: Vec<String> = env::args().collect();
    let id = args.get(1).unwrap().parse().unwrap();

    let addr = "127.0.0.1:9142".parse().unwrap();
    let self_id: ReactorId = rand::thread_rng().gen();

    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new().unwrap();
        let reactor = ClientReactor {
            connected: false,
            id,
        };
        broker.spawn(self_id.clone(), reactor.params(), "main");

        tokio::spawn(server::connect_to_server(broker, self_id, &addr));

        Ok(())
    }));
}

// Main client logic
/// ? greeter_id is the server, the tcp stream that you connected to
/// ? runtime_id is your own runtime, to handle visualisation etc
/// ? user are you  ...
struct ClientReactor {
    connected: bool,
    id: u64,
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
        // open link with runtime, for communicating with chat GUI
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

        if !self.connected {
            handle.open_link(ServerLink::params(id.into()))?;
            self.connected = true;

            let mut identify = MsgBuffer::<identify::Owned>::new();
            identify.build(|b| {
                b.set_key(self.id);
            });
            handle.send_internal(identify).display();

        } else {
            handle.open_link(HostLink::params(ReactorId::from(id)))?;
        }


        Ok(())
    }
}

// Handler for the connection with the chat server
struct ServerLink;
impl ServerLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);

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

struct HostLink;

impl HostLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, HostLink);

        params.external_handler(
            host_message::Owned,
            CtxHandler::new(Self::receive_chat_message),
        );

        params.internal_handler(
            chat_capnp::send_message::Owned,
            CtxHandler::new(Self::send_chat_message),
        );

        params.external_handler(
            client_kicked::Owned,
            CtxHandler::new(Self::client_kicked),
        );

        return params;
    }

    // pick up a 'send_message' event from the reactor, and put it to effect
    // by constructing the chat message and sending it to the chat server.
    fn send_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        send_message: chat_capnp::send_message::Reader,
    ) -> Result<()>
    {
        let message = send_message.get_message()?;
        // let user = send_message.get_user()?;

        let mut chat_message = MsgBuffer::<client_message::Owned>::new();
        chat_message.build(|b| {
            b.set_data(message.as_bytes());
        });

        handle.send_message(chat_message)?;

        return Ok(());
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
        handle: &mut LinkHandle<C>,
        chat_message: host_message::Reader,
    ) -> Result<()>
    {
        let message = chat_message.get_data()?;

        println!("{}", str::from_utf8(message).unwrap());

        return Ok(());
    }
}

//
struct RuntimeLink;

impl RuntimeLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);

        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(actor_joined::e_to_i),
        );

        return params;
    }
}
