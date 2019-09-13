use std::env;

extern crate serde_json;
extern crate tokio;
extern crate futures;
extern crate mozaic;
extern crate rand;
extern crate capnp;

use std::net::SocketAddr;
use mozaic::core_capnp::{initialize, set_timeout};
use mozaic::messaging::types::*;
use mozaic::messaging::reactor::*;
use mozaic::errors;

use mozaic::modules::log_reactor;
use mozaic::modules::{Aggregator, Steplock};
use mozaic::client_capnp::{client_turn, host_message, to_client, client_step};

pub mod chat {
    include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
}

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}


struct Welcomer {
    aggregator: ReactorId,
}

impl Welcomer {
    fn params<C: Ctx>(aggregator: ReactorId) -> CoreParams<Self, C> {
        let me = Self {
            aggregator,
        };

        let mut params = CoreParams::new(me);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(
            client_step::Owned,
            CtxHandler::new(Self::handle_step)
        );

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), errors::Error>
    {
        handle.open_link(ClientLink::params(self.aggregator.clone()))?;

        return Ok(());
    }

    fn handle_step<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msgs: client_step::Reader,
    ) -> Result<(), errors::Error>
    {
        for reader in msgs.get_data()?.iter() {
            let user = reader.get_client_id();

            let message = match reader.which()? {
                client_turn::Which::Timeout(_) => {
                    format!("Client {} sent {}", user, "timeout")
                },
                client_turn::Which::Turn(turn) => {
                    let message = String::from_utf8(turn?.to_vec()).unwrap();
                    format!("Client {} sent {}", user, message)
                }
            };

            log_reactor(handle, &message);


            let mut chat_message = MsgBuffer::<host_message::Owned>::new();
            chat_message.build(|b| {
                b.set_data(message.as_bytes());
            });
            handle.send_internal(
                chat_message
            )?;
        }

        let timeout = MsgBuffer::<set_timeout::Owned>::new();
        handle.send_internal(timeout)?;

        return Ok(());
    }
}


/// Link with client controller
struct ClientLink;
impl ClientLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ClientLink);

        params.internal_handler(
            host_message::Owned,
            CtxHandler::new(host_message::i_to_e),
        );

        params.internal_handler(
            to_client::Owned,
            CtxHandler::new(to_client::i_to_e),
        );

        params.internal_handler(
            set_timeout::Owned,
            CtxHandler::new(set_timeout::i_to_e),
        );

        params.external_handler(
            client_step::Owned,
            CtxHandler::new(client_step::e_to_i),
        );

        return params;
    }
}

use mozaic::server::runtime::{Broker};
use rand::Rng;
use errors::Consumable;
use mozaic::modules::ConnectionManager;
use mozaic::modules::util;
use std::collections::HashMap;

pub fn run(args : Vec<String>) {

    let addr = "127.0.0.1:9142".parse::<SocketAddr>().unwrap();

    let manager_id: ReactorId = rand::thread_rng().gen();
    let welcomer_id: ReactorId = rand::thread_rng().gen();
    let aggregator_id: ReactorId = rand::thread_rng().gen();
    let steplock_id: ReactorId = rand::thread_rng().gen();

    let number_of_clients = args.get(1).map(|x| x.parse().unwrap_or(1)).unwrap_or(1);

    let ids: HashMap<_, util::PlayerId> = (0..number_of_clients).map(|x| (x.into(), (10 - x).into())).collect();

    println!("Ids: {:?}", ids);

    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new().unwrap();

        broker.spawn(welcomer_id.clone(), Welcomer::params(steplock_id.clone()), "Main").display();
        broker.spawn(steplock_id.clone(), Steplock::new(broker.clone(), ids.values().cloned().collect(), welcomer_id.clone(), aggregator_id.clone()).with_timeout(5000).params(), "Steplock").display();
        broker.spawn(aggregator_id.clone(), Aggregator::params(manager_id.clone(), steplock_id.clone()), "Aggregator").display();
        broker.spawn(
            manager_id.clone(),
            ConnectionManager::params(broker.clone(), ids, aggregator_id.clone(), addr),
            "Connection Manager"
        ).display();

        Ok(())
    }));
}
