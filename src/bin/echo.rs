extern crate futures;
extern crate mozaic;
extern crate mozaic_derive;
#[macro_use]
extern crate tokio;
extern crate serde;
extern crate serde_json;

extern crate tracing;
extern crate tracing_subscriber;

use tracing_subscriber::{EnvFilter, FmtSubscriber};

use std::{time};

use mozaic::generic::*;

use mozaic::modules::types::*;
use mozaic::modules::{
    Aggregator, ClientController, ConnectionManager, GameController, GameRunner, StepLock,
};

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

async fn run(pool: ThreadPool) {
    use std::collections::HashMap;

    let broker = BrokerHandle::new(pool.clone());
    let json_broker = BrokerHandle::new(pool.clone());

    let player_ids = vec![10, 11];
    let cc_ids = vec![11.into(), 12.into()];

    let player_map: HashMap<PlayerId, ReactorID> = player_ids
        .iter()
        .cloned()
        .zip(cc_ids.iter().cloned())
        .collect();

    let echo_id = 0.into();
    let step_id = 2.into();
    let agg_id = 1.into();
    let cm_id = 100.into();

    let echo = GameRunner::params(
        step_id,
        Echo {
            clients: player_ids.clone(),
        },
    );
    let agg = Aggregator::params(step_id, player_map.clone());
    let step = StepLock::params(echo_id, agg_id, player_ids, Some(3000));

    let cm = ConnectionManager::params(
        pool.clone(),
        "127.0.0.1:6666".parse().unwrap(),
        json_broker.clone(),
        player_map.clone(),
    );

    player_map
        .iter()
        .map(|(&id, &r_id)| {
            ClientController::new(r_id, json_broker.clone(), broker.clone(), agg_id, cm_id, id)
        })
        .for_each(|cc| pool.spawn_ok(cc));

    join!(
        broker.spawn_with_handle(echo, Some(echo_id)).0,
        broker.spawn_with_handle(agg, Some(agg_id)).0,
        broker.spawn_with_handle(step, Some(step_id)).0,
        json_broker.spawn_with_handle(cm, Some(cm_id)).0,
    );
}

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

        run(pool).await;
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
