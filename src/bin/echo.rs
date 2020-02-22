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
use futures::future::FutureExt;

struct Echo {
    clients: Vec<PlayerId>,
}

impl GameController for Echo {
    fn step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg> {
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

use mozaic::generic::*;
use mozaic::graph;
use mozaic::modules::*;

#[tokio::main]
async fn main() {
    let fut = graph::set_default();

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
        pool.spawn_ok(fut.map(|_| ()));

        let (broker, handle) = BrokerHandle::new(pool.clone());

        let gm_id = ReactorID::rand();
        let cm_id = ReactorID::rand();
        let ep_id = ReactorID::rand();

        let mut gm = GameManager::new(broker.clone(), gm_id, cm_id, pool.clone());
        let cm_params = ClientManager::new(gm_id, vec![ep_id]);
        broker.spawn(cm_params, Some(cm_id));

        let (tp_tx, tp_fut) = TcpEndpoint::new(ep_id, "127.0.0.1:6666".parse().unwrap(), broker.get_sender(&cm_id), pool.clone());

        broker.spawn_reactorlike(ep_id, tp_tx, tp_fut, "TCP endpoint");

        let players = vec![10, 11];
        let game = Echo {
            clients: players.clone(),
        };

        let builder = GameBuilder::new(players.clone(), game).with_step_lock(
            StepLock::new(players.clone(), pool.clone()),
        );

        let game_id = gm.start_game(builder).await.unwrap();

        println!("State: {:?}", gm.get_state(game_id).await);

        handle.await;
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
