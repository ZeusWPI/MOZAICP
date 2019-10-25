
extern crate tokio;
extern crate futures;
extern crate mozaic;
extern crate rand;

extern crate tracing;
extern crate tracing_futures;
extern crate tracing_subscriber;

use tracing::{span, Level};
use tracing_futures::Instrument;
use tracing_subscriber::{EnvFilter, fmt};

use std::env;
use std::net::SocketAddr;
use mozaic::messaging::types::*;

use mozaic::modules::{Aggregator, game};

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}

use std::str;
struct Server;
impl game::GameController for Server {
    fn step<'a>(&mut self, turns: Vec<game::PlayerTurn<'a>>) -> Vec<game::Update> {
        let mut out = Vec::new();

        for (id, turn) in turns.iter() {
            let postfix = match turn {
                game::Turn::Action(bytes) => str::from_utf8(bytes).unwrap(),
                game::Turn::Timeout => "Timed out",
            };

            let msg = format!("{}: {}", **id, postfix);

            out.push(game::Update::Global(msg.as_bytes().to_vec()));

            if postfix == "stop" {
                out.push(game::Update::Kick(*id));
            }
        }

        return out;
    }
}

use mozaic::runtime::{Broker};
use rand::Rng;
use mozaic::errors::Consumable;
use mozaic::modules::ConnectionManager;
use mozaic::modules::util;
use std::collections::HashMap;

pub fn run(args : Vec<String>) {

    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .inherit_fields(true)
        .with_max_level(Level::DEBUG)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let addr = "127.0.0.1:9142".parse::<SocketAddr>().unwrap();

    let manager_id: ReactorId = rand::thread_rng().gen();
    let welcomer_id: ReactorId = rand::thread_rng().gen();
    let aggregator_id: ReactorId = rand::thread_rng().gen();
    // let steplock_id: ReactorId = rand::thread_rng().gen();

    let number_of_clients = args.get(1).map(|x| x.parse().unwrap_or(1)).unwrap_or(1);

    let ids: HashMap<_, util::PlayerId> = (0..number_of_clients).map(|x| (x.into(), (10 - x).into())).collect();

    println!("Ids: {:?}", ids);

    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new().unwrap();

        broker.spawn(welcomer_id.clone(), game::GameReactor::params(aggregator_id.clone(), Box::new(Server)), "Server").display();
        // broker.spawn(steplock_id.clone(), Steplock::new(broker.clone(), ids.values().cloned().collect(), welcomer_id.clone(), aggregator_id.clone()).with_timeout(5000).with_initial_timeout(500).params(), "Steplock").display();
        broker.spawn(aggregator_id.clone(), Aggregator::params(manager_id.clone(), welcomer_id.clone()), "Aggregator").display();
        broker.spawn(
            manager_id.clone(),
            ConnectionManager::params(broker.clone(), ids, aggregator_id.clone(), addr),
            "Connection Manager"
        ).display();

        Ok(())
    }).instrument(span!(Level::TRACE, "main")));
}
