use std::env;

extern crate sodiumoxide;
extern crate serde_json;
extern crate tokio;
extern crate futures;
extern crate mozaic;
extern crate rand;
extern crate capnp;

use std::net::SocketAddr;
use mozaic::core_capnp::{initialize, actor_joined};
use mozaic::messaging::types::*;
use mozaic::messaging::reactor::*;
use mozaic::server::run_server;

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
        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        let link = WelcomerRuntimeLink {};
        handle.open_link(link.params(self.runtime_id.clone()));

        let logger = Logger { from: handle.id().clone()};
        let loggerId = handle.spawn(logger.params());

        let loggerLink = LoggerLink {};
        handle.open_link(loggerLink.params(loggerId));
        return Ok(());
    }

    fn handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), capnp::Error>
    {
        /// ? id is the id of the client reactor on the other side of the interwebs
        let id: ReactorId = r.get_id()?.into();
        println!("welcoming {:?}", id);
        let link = WelcomerGreeterLink {};
        handle.open_link(link.params(id));
        return Ok(());
    }

    fn handle_chat_message<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        msg: chat::chat_message::Reader,
    ) -> Result<(), capnp::Error>
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

    /// ? Create a new handle connecting to the new users's reactor
    fn e_handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), capnp::Error>
    {
        let id: ReactorId = r.get_id()?.into();

        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id.bytes()));
        handle.send_internal(joined);
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
        params.internal_handler(
            chat::chat_message::Owned,
            CtxHandler::new(Self::i_handle_chat_msg_send),
        );

        return params;
    }

    fn i_handle_chat_msg_send<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: chat::chat_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let content = message.get_message()?;
        let user = message.get_user()?;

        let mut chat_message = MsgBuffer::<chat::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(content);
            b.set_user(user);
        });
        handle.send_message(chat_message);

        return Ok(());
    }

    fn e_handle_chat_msg_resv<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: chat::chat_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let content = message.get_message()?;
        let user = message.get_user()?;

        let mut chat_message = MsgBuffer::<chat::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(content);
            b.set_user(user);
        });

        handle.send_internal(chat_message);

        return Ok(());
    }
}

// This is aboslutely empty
struct Logger {
    from: ReactorId,
}

impl Logger {
    fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::initialize));
        return params;
    }

    // reactor setup
    fn initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        let link = LoggerLink{};
        handle.open_link(link.params(self.from.clone()));

        Ok(())
    }
}

struct LoggerLink {

}

impl LoggerLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {

        let mut params = LinkParams::new(foreign_id, self);

        params.external_handler(
            chat::user_input::Owned,
            CtxHandler::new(Self::handle_user_input),
        );

        params.internal_handler(
            chat::chat_message::Owned,
            CtxHandler::new(Self::log),
        );

        return params;
    }

    fn log<C: Ctx> (
        &mut self,
        handle: &mut LinkHandle<C>,
        input: chat::chat_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let message = input.get_message()?;

        let mut send_message = MsgBuffer::<chat::user_input::Owned>::new();
        send_message.build(|b| {
            b.set_text(message);
        });

        handle.send_message(send_message);
        return Ok(());
    }

    fn handle_user_input<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        input: chat::user_input::Reader,
    ) -> Result<(), capnp::Error>
    {
        let message = input.get_text()?;

        let mut send_message = MsgBuffer::<chat::send_message::Owned>::new();
        send_message.build(|b| {
            b.set_message(message);
            b.set_user(&"Bob");
        });

        handle.send_internal(send_message);

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
