use std::env;

extern crate serde_json;
extern crate tokio;
extern crate futures;
extern crate mozaic;
extern crate rand;
extern crate capnp;

use std::net::SocketAddr;
use mozaic::core_capnp::{initialize, actor_joined};
use mozaic::network_capnp::{disconnected};
use mozaic::messaging::types::*;
use mozaic::messaging::reactor::*;
use mozaic::server::run_server;
use mozaic::errors;

use mozaic::modules::log_reactor;

// TODO: Find from where to get disconnect event something something

pub mod chat {
    include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
}

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}


struct Welcomer {
    runtime_id: ReactorId,
}

impl Welcomer {
    fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::handle_actor_joined));
        params.handler(
            chat::chat_message::Owned,
            CtxHandler::new(Self::handle_chat_message)
        );

        params.handler(disconnected::Owned, CtxHandler::new(Self::handle_disconnected));
        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), errors::Error>
    {
        let link = WelcomerRuntimeLink {};
        handle.open_link(link.params(self.runtime_id.clone()))?;

        return Ok(());
    }

    fn handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), errors::Error>
    {
        //? id is the id of the client reactor on the other side of the interwebs
        let id: ReactorId = r.get_id()?.into();
        println!("welcoming {:?}", id);

        let link = WelcomerGreeterLink {};
        handle.open_link(link.params(id))?;
        return Ok(());
    }

    fn handle_disconnected<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: disconnected::Reader,
    ) -> Result<(), errors::Error> {

        let id = r.get_id()?;

        log_reactor(handle, &format!("Client {:?} disconnected", id));

        Ok(())
    }

    fn handle_chat_message<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        msg: chat::chat_message::Reader,
    ) -> Result<(), errors::Error>
    {
        println!("{}: {}",msg.get_user()? ,msg.get_message()?);
        return Ok(());
    }
}

struct WelcomerRuntimeLink {}

impl WelcomerRuntimeLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(Self::e_handle_actor_joined),
        );

        return params;
    }

    //? Retransmit actor_joined so the Welcomer can create a link to it.
    fn e_handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), errors::Error>
    {
        let id: ReactorId = r.get_id()?.into();

        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id.bytes()));
        handle.send_internal(joined)?;
        return Ok(());
    }
}

struct WelcomerGreeterLink {}

impl WelcomerGreeterLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            chat::chat_message::Owned,
            CtxHandler::new(Self::e_handle_chat_msg_resv),
        );

        params.external_handler(
            disconnected::Owned,
            CtxHandler::new(Self::e_handle_disconnected),
        );

        params.internal_handler(
            chat::chat_message::Owned,
            CtxHandler::new(Self::i_handle_chat_msg_send),
        );

        return params;
    }

    //? Listen for local chat messages and resend them trough the interwebs
    fn i_handle_chat_msg_send<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: chat::chat_message::Reader,
    ) -> Result<(), errors::Error>
    {
        let content = message.get_message()?;
        let user = message.get_user()?;

        let mut chat_message = MsgBuffer::<chat::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(content);
            b.set_user(user);
        });

        handle.send_message(chat_message)?;

        return Ok(());
    }

    //? Handle chat messages by sending them internally so everybody can hear them
    //? Including you self
    fn e_handle_chat_msg_resv<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: chat::chat_message::Reader,
    ) -> Result<(), errors::Error>
    {
        let content = message.get_message()?;
        let user = message.get_user()?;

        let mut chat_message = MsgBuffer::<chat::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(content);
            b.set_user(user);
        });

        handle.send_internal(chat_message)?;

        return Ok(());
    }

    fn e_handle_disconnected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: disconnected::Reader,
    ) -> Result<(), errors::Error>
    {
        let id = message.get_id()?;

        let mut disconnected = MsgBuffer::<disconnected::Owned>::new();
        disconnected.build(|b| {
            b.set_id(id);
        });

        eprintln!("Im HERE FIRST");


        handle.send_internal(disconnected)?;
        eprintln!("Im HERE");

        handle.close_link()?;
        eprintln!("Closed link");

        return Ok(());
    }
}

pub fn run(_args : Vec<String>) {

    let addr = "127.0.0.1:9142".parse::<SocketAddr>().unwrap();

    run_server(addr, |runtime_id| {
        let w = Welcomer { runtime_id };
        return w.params();
    });
}
