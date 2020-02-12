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

struct Echo {
    clients: Vec<PlayerId>,
}

impl GameController for Echo {
    fn step<'a>(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg> {
        let mut sub = Vec::new();
        for PlayerMsg { id, data } in turns {
            let msg = data.map(|x| x.value).unwrap_or(String::from("TIMEOUT"));
            if "stop".eq_ignore_ascii_case(&msg) {
                sub.push(HostMsg::kick(id));
            }

            for target in &self.clients {
                sub.push(HostMsg::Data(
                    Data {
                        value: format!("{}: {}\n", id, msg),
                    },
                    Some(*target),
                ));
            }
        }

        sub
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

        let players = vec![10, 11];
        let game = Echo {
            clients: players.clone(),
        };
        let builder = GameBuilder::new(players.clone(), game)
            .with_step_lock(StepLock::new(players.clone()).with_timeout(3000));

        builder.run(pool).await;
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
