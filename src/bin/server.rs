use std::env;

extern crate serde_json;
extern crate tokio;
extern crate futures;
extern crate mozaic;
extern crate rand;
extern crate capnp;

use std::net::SocketAddr;
use mozaic::core_capnp::{initialize};
use mozaic::messaging::types::*;
use mozaic::messaging::reactor::*;
use mozaic::errors;

use mozaic::modules::log_reactor;
use mozaic::client_capnp::{from_client, host_message};

// TODO: Find from where to get disconnect event something something

pub mod chat {
    include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
}

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}


struct Welcomer {
    connection_id: ReactorId,
}

impl Welcomer {
    fn params<C: Ctx>(connection_id: ReactorId) -> CoreParams<Self, C> {
        let me = Self {
            connection_id,
        };

        let mut params = CoreParams::new(me);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(
            from_client::Owned,
            CtxHandler::new(Self::handle_chat_message)
        );

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), errors::Error>
    {
        let link = WelcomerConnectionLink {};

        handle.open_link(link.params(self.connection_id.clone()))?;

        return Ok(());
    }

    fn handle_chat_message<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: from_client::Reader,
    ) -> Result<(), errors::Error>
    {
        println!("host handling chat message");
        let user = msg.get_client_id();
        let message = msg.get_data()?;

        let message = format!("Client {} sent {}", user, message);

        log_reactor(handle, &message);
        println!("{}", message);

        if !message.contains("kaka") {

            let mut chat_message = MsgBuffer::<host_message::Owned>::new();
            chat_message.build(|b| {
                b.set_data(&message);
            });
            handle.send_internal(
                chat_message
            )?;
        }

        return Ok(());
    }
}

struct WelcomerConnectionLink {}

impl WelcomerConnectionLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            from_client::Owned,
            CtxHandler::new(Self::e_handle_client_message),
        );

        params.internal_handler(
            host_message::Owned,
            CtxHandler::new(Self::i_handle_chat_msg_send),
        );

        return params;
    }

    //? Listen for local chat messages and resend them trough the interwebs
    fn i_handle_chat_msg_send<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: host_message::Reader,
    ) -> Result<(), errors::Error>
    {
        let content = message.get_data()?;

        let mut chat_message = MsgBuffer::<host_message::Owned>::new();
        chat_message.build(|b| {
            b.set_data(content);
        });

        handle.send_message(chat_message)?;

        return Ok(());
    }

    //? Handle chat messages by sending them internally so everybody can hear them
    //? Including you self
    fn e_handle_client_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: from_client::Reader,
    ) -> Result<(), errors::Error>
    {
        let content = message.get_data()?;
        let user = message.get_client_id();

        let mut chat_message = MsgBuffer::<from_client::Owned>::new();
        chat_message.build(|b| {
            b.set_data(content);
            b.set_client_id(user);
        });

        println!("host got msg {}", content);

        handle.send_internal(chat_message)?;

        return Ok(());
    }
}

use mozaic::server::runtime::{Broker};
use rand::Rng;
use errors::Consumable;
use mozaic::modules::ConnectionManager;

pub fn run(args : Vec<String>) {

    let addr = "127.0.0.1:9142".parse::<SocketAddr>().unwrap();

    let manager_id: ReactorId = rand::thread_rng().gen();
    let welcomer_id: ReactorId = rand::thread_rng().gen();

    let number_of_clients = args.get(1).map(|x| x.parse().unwrap_or(1)).unwrap_or(1);

    let ids = (0..number_of_clients).map(|x| (x.into(), x.into())).collect();

    println!("Ids: {:?}", ids);

    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new().unwrap();

        broker.spawn(welcomer_id.clone(), Welcomer::params(manager_id.clone()), "Main").display();
        broker.spawn(
            manager_id.clone(),
            ConnectionManager::params(broker.clone(), ids, welcomer_id.clone(), addr),
            "Connection Manager").display();

        Ok(())
    }));

}
