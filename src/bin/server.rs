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
use mozaic::client_capnp::{from_client, host_message, to_client};
use mozaic::core_capnp::actors_joined;

pub mod chat {
    include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
}

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}


struct Welcomer {
    connection_manager: ReactorId,
}

impl Welcomer {
    fn params<C: Ctx>(connection_manager: ReactorId) -> CoreParams<Self, C> {
        let me = Self {
            connection_manager,
        };

        let mut params = CoreParams::new(me);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(
            from_client::Owned,
            CtxHandler::new(Self::handle_chat_message)
        );
        params.handler(
            actors_joined::Owned,
            CtxHandler::new(Self::handle_actors_joined),
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

        handle.open_link(link.params(self.connection_manager.clone()))?;

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

    fn handle_actors_joined<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: actors_joined::Reader,
    ) -> Result<(), errors::Error>
    {
        for id in msg.get_ids()?.iter() {
            let id = id.unwrap();
            let client_id = ReactorId::from(id);
            handle.open_link(ClientLink::params(client_id)).unwrap();
        }
        Ok(())
    }
}

struct WelcomerConnectionLink {}

impl WelcomerConnectionLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            actors_joined::Owned,
            CtxHandler::new(Self::e_handle_actors_joined),
        );

        return params;
    }

    fn e_handle_actors_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actors_joined::Reader,
    ) -> errors::Result<()> {
        let ids = r.get_ids()?;

        let mut joined = MsgBuffer::<actors_joined::Owned>::new();
        joined.build(|b| b.set_ids(ids).expect("Fuck off"));
        handle.send_internal(joined)?;

        Ok(())
    }
}

/// Link with client controller
struct ClientLink;
impl ClientLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ClientLink);

        params.internal_handler(
            host_message::Owned,
            CtxHandler::new(Self::i_handle_host_msg),
        );

        params.internal_handler(
            to_client::Owned,
            CtxHandler::new(Self::i_handle_to_client),
        );

        params.external_handler(
            from_client::Owned,
            CtxHandler::new(Self::e_handle_from_client),
        );

        return params;
    }

    fn i_handle_host_msg<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: host_message::Reader,
    ) -> errors::Result<()> {
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<host_message::Owned>::new();
        joined.build(|b| b.set_data(msg));
        handle.send_message(joined)?;

        Ok(())
    }

    fn i_handle_to_client<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: to_client::Reader,
    ) -> errors::Result<()> {
        let id = r.get_client_id();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<to_client::Owned>::new();
        joined.build(|b| {
            b.set_data(msg);
            b.set_client_id(id);
        });
        handle.send_message(joined)?;

        Ok(())
    }

    fn e_handle_from_client<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: from_client::Reader,
    ) -> errors::Result<()> {
        let id = r.get_client_id();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<from_client::Owned>::new();
        joined.build(|b| {
            b.set_data(msg);
            b.set_client_id(id);
        });
        handle.send_internal(joined)?;

        Ok(())
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

    let ids = (0..number_of_clients).map(|x| (x.into(), (10 - x).into())).collect();

    println!("Ids: {:?}", ids);

    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new().unwrap();

        broker.spawn(welcomer_id.clone(), Welcomer::params(manager_id.clone()), "Main").display();
        broker.spawn(
            manager_id.clone(),
            ConnectionManager::params(broker.clone(), ids, welcomer_id.clone(), addr),
            "Connection Manager"
        ).display();

        Ok(())
    }));

}
