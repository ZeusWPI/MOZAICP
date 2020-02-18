extern crate futures;
extern crate mozaic;
extern crate rand;
extern crate tokio;

extern crate tracing;
extern crate tracing_futures;
extern crate tracing_subscriber;

use tracing::{span, Level};
use tracing_futures::Instrument;
use tracing_subscriber::{fmt, EnvFilter};

use mozaic::messaging::types::*;
use mozaic::graph::Graph;

use std::env;
use std::net::SocketAddr;

use mozaic::modules::{game, Aggregator};

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}

use std::str;
struct Server {
    next_player: u64,
    field: Vec<Vec<u64>>
}
impl game::GameController for Server {
    fn step<'a>(&mut self, turns: Vec<game::PlayerTurn<'a>>) -> Vec<game::Update> {
        let updates = Vec::new();
        if let Some((id, game::Turn::Action(bytes))) = turns.get(0) {
            let id: u64 = id.into();
            if self.next_player == id {
                let message = str::from_utf8(bytes).expect("Could not translate bytes into string");
                let column: u32 = match message.parse() {
                    Ok(num) => num,
                    Err(_) => return vec![game::Update::Kick(id.into())],
                };
                
                if self.field[column as usize].len() < 7 {
                    self.field[column as usize].push(id)
                }
                if check_if_won(self.field) {
                    updates.push(game::Update::Player(self.next_player.into(), String::from("You Win").into_bytes()));
                    updates.push(game::Update::Player((1 - self.next_player).into(), String::from("You Lose").into_bytes()));
                    updates.push(game::Update::Kick(0.into()));
                    updates.push(game::Update::Kick(1.into()));
                    updates
                } else {
                    updates.push(game::Update::Global(String::from("Ok").into_bytes()));
                }
            }
        }

        updates
    }
}

fn check_if_won(field: Vec<Vec<u64>>) -> bool {
    //max line length
    let max_length = field.iter().map(|x| x.len()).max().expect("No max?");
    let mut history = 0;
    let mut last = 999;
    // horizontal
    for linei in 0..max_length {
        for column in &field {
            if linei < column.len() {
                let value = column[linei];
                if last != value {
                    history = 0;
                    last = value;
                } else {
                    history += 1;
                }
                if history == 3 {
                    return true;
                }
                
            } else {
                history = 0;
                last = 999;
            }
        }
    }
    //vertical
    history = 0;
    last = 999;
    for column in &field {
        if column.len() >= 4 {
            for value in column.iter() {
                if last != *value {
                    history = 0;
                    last = *value;
                } else {
                    history += 1;
                }
                if history == 3 {
                    return true;
                }
            }
        }
    }
    
    //diagonal
    if max_lenth >= 4 {
        
    }
    
    false
}

impl Server {
    fn new() -> Server {
        let rand_next_player = if rand::random() {
            0
        } else {
            1
        };

        let mut empty_field = Vec::new();
        
        for i in 0..8 {
            empty_field.push(Vec::new());
        }

        Server{next_player: rand_next_player, field: empty_field}
    }
}

use mozaic::errors::Consumable;
use mozaic::modules::util;
use mozaic::modules::ConnectionManager;
use mozaic::runtime::Broker;
use mozaic::graph;
use rand::Rng;
use std::collections::HashMap;

pub fn run(args: Vec<String>) {
    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::from("info")))
        .without_time()
        .inherit_fields(true)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let addr = "127.0.0.1:9142".parse::<SocketAddr>().unwrap();

    let manager_id: ReactorId = rand::thread_rng().gen();
    let welcomer_id: ReactorId = rand::thread_rng().gen();
    let aggregator_id: ReactorId = rand::thread_rng().gen();
    // let steplock_id: ReactorId = rand::thread_rng().gen();

    let number_of_clients = 2;

    let ids: HashMap<_, util::PlayerId> = (0..number_of_clients)
        .map(|x| (x.into(), (10 - x).into()))
        .collect();

    println!("Ids: {:?}", ids);

    tokio::run(
        futures::lazy(move || {
            graph::set_graph(Graph::new());
            let mut broker = Broker::new().unwrap();

            broker
                .spawn(
                    welcomer_id.clone(),
                    game::GameReactor::params(aggregator_id.clone(), Box::new(Server{next_player: 0, field: Vec::new()})),
                    "Server",
                )
                .display();
            // broker.spawn(steplock_id.clone(), Steplock::new(broker.clone(), ids.values().cloned().collect(), welcomer_id.clone(), aggregator_id.clone()).with_timeout(5000).with_initial_timeout(500).params(), "Steplock").display();
            broker
                .spawn(
                    aggregator_id.clone(),
                    Aggregator::params(manager_id.clone(), welcomer_id.clone()),
                    "Aggregator",
                )
                .display();
            broker
                .spawn(
                    manager_id.clone(),
                    ConnectionManager::params(broker.clone(), ids, aggregator_id.clone(), addr),
                    "Connection Manager",
                )
                .display();

            Ok(())
        })
        .instrument(span!(Level::TRACE, "main")),
    );
}
