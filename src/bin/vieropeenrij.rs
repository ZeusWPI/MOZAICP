extern crate futures;
extern crate mozaic;
extern crate mozaic_derive;
extern crate serde;
extern crate serde_json;
extern crate tokio;

extern crate tracing;
extern crate tracing_subscriber;


use tracing_subscriber::{EnvFilter, FmtSubscriber};

use std::time;

use mozaic::modules::types::*;
use mozaic::modules::{GameController, StepLock};

use futures::executor::ThreadPool;

use std::env;
use std::net::SocketAddr;

use mozaic::modules::{Aggregator};

use std::str;
struct Server {
    next_player: u64,
    field: Vec<Vec<u64>>
}
impl GameController for Server {
    fn step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg> {
        let mut updates = Vec::new();
        if let Some(PlayerMsg { id, data: Some(data) }) = turns.get(0) {
            println!("HERE");
            if self.next_player == *id {
                // let message = str::from_utf8(bytes).expect("Could not translate bytes into string");
                let column: u32 = match data.value.parse() {
                    Ok(num) => num,
                    Err(_) => return vec![HostMsg::Kick(*id)],
                };
                
                if self.field[column as usize].len() < 7 {
                    if self.next_player == 0 {
                        self.next_player = 1;
                    } else {
                        self.next_player = 0;
                    }
                    self.field[column as usize].push(*id)
                }
                if check_if_won(&self.field) {
                    updates.push(HostMsg::new(String::from("You Win"), Some(self.next_player)));
                    updates.push(HostMsg::new(String::from("You Lose"), Some(1 - self.next_player)));
                    updates.push(HostMsg::Kick(0));
                    updates.push(HostMsg::Kick(1));
                } else {
                    updates.push(HostMsg::new(String::from("Ok"), None));
                }
            }
        }

        updates
    }
}

fn check_if_won(field: &Vec<Vec<u64>>) -> bool {
    //max line length
    let max_length = field.iter().map(|x| x.len()).max().expect("No max?");
    let mut history = 0;
    let mut last = 999;
    // horizontal
    for linei in 0..max_length {
        for column in field {
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
    for column in field {
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
    
    // //diagonal
    // if max_lenth >= 4 {
        
    // }
    
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

use mozaic::modules::GameBuilder;
#[tokio::main]
async fn main() {
    let sub = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(sub).unwrap();
    {
        let pool = ThreadPool::builder()
            // .after_start(|i| println!("Starting thread {}", i))
            // .before_stop(|i| println!("Stopping thread {}", i))
            .create()
            .unwrap();

        let players = vec![0, 1];
        let game = Server::new();
        let builder = GameBuilder::new(players.clone(), game);
            // .with_step_lock(StepLock::new(players.clone()).with_timeout(3000));

        builder.run(pool).await;
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
