extern crate async_std;
extern crate futures;
extern crate mozaic;

#[macro_use]
extern crate serde_json;

extern crate tracing;
extern crate tracing_subscriber;

use tracing_subscriber::{EnvFilter, FmtSubscriber};

use std::time;
use serde_json::Value;

use mozaic::modules::types::*;
use mozaic::modules::{game};

use futures::executor::ThreadPool;
use futures::future::FutureExt;

#[derive(Clone)]
struct Echo {
    clients: Vec<PlayerId>,
}

impl game::Controller for Echo {
    fn step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg> {
        let mut sub = Vec::new();
        for PlayerMsg { id, data } in turns {
            let msg = data.map(|x| x.value).unwrap_or(String::from("TIMEOUT"));
            if "stop".eq_ignore_ascii_case(&msg) {
                sub.push(HostMsg::kick(id));
                self.clients = self.clients.iter().cloned().filter(|&x| x != id).collect();
                println!("{:?}", self.clients);
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

    fn is_done(&mut self) -> Option<(String, Value)> {
        if self.clients.is_empty() {
            let value = json!({"testing": 123});
            Some(("Echo game".to_string(), value))
        } else {
            None
        }
    }
}

use mozaic::graph;
use mozaic::modules::net::TcpEndpoint;

use std::collections::VecDeque;

#[async_std::main]
async fn main() -> std::io::Result<()> {
    let fut = graph::set_default();

    let sub = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(sub).unwrap();
    {
        let pool = ThreadPool::builder().create().unwrap();
        pool.spawn_ok(fut.map(|_| ()));

        let (gmb, handle) = game::Manager::builder(pool.clone());
        let ep = TcpEndpoint::new("127.0.0.1:6666".parse().unwrap(), pool.clone());

        let gmb = gmb.add_endpoint(ep, "TCP endpoint");
        let gm = gmb.build("game.ini", pool.clone()).await.unwrap();

        let mut games = VecDeque::new();

        let game_builder = {
            let players = vec![10];
            let game = Echo {
                clients: players.clone(),
            };

            game::Builder::new(players.clone(), game)
        };
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());
        println!("{:?}", gm.get_state(*games.back().unwrap()).await);

        loop {
            async_std::task::sleep(std::time::Duration::from_millis(3000)).await;

            match gm.get_state(*games.back().unwrap()).await {
                Some(Ok(v)) => println!("{:?}", v),
                Some(Err(e)) => {println!("{:?}", e); break },
                None => {}
            }
        }

        handle.await;
    }

    std::thread::sleep(time::Duration::from_millis(100));

    Ok(())
}
