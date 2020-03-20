extern crate async_std;
extern crate futures;
extern crate mozaic;

extern crate tracing;
extern crate tracing_subscriber;

use tracing_subscriber::{EnvFilter, FmtSubscriber};

use std::time;

use mozaic::modules::types::*;
use mozaic::modules::{GameController, StepLock, GameManagerBuilder};

use futures::executor::ThreadPool;
use futures::future::FutureExt;

#[derive(Clone)]
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
                self.clients = self.clients.iter().cloned().filter(|&x| x == id).collect();
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

    fn is_done(&mut self) -> bool {
        self.clients.is_empty()
    }
}

use mozaic::graph;
use mozaic::modules::*;

use std::collections::VecDeque;

#[async_std::main]
async fn main() -> std::io::Result<()> {
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

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;

        let (gmb, handle) = GameManagerBuilder::new(pool.clone());
        let ep = TcpEndpoint::new(
            "127.0.0.1:6666".parse().unwrap(),
            pool.clone(),
        );

        let gmb = gmb.add_endpoint(ep, "TCP endpoint");
        let mut gm = gmb.build();

        let mut games = VecDeque::new();

        let game_builder = {
            let players = vec![10, 11, 12, 13];
            let game = Echo {
                clients: players.clone(),
            };

            GameBuilder::new(players.clone(), game).with_step_lock(
                StepLock::new(players.clone(), pool.clone())
                    .with_timeout(std::time::Duration::from_secs(5)),
            )
        };

        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());
        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        gm.kill_game(games.pop_front().unwrap()).await.unwrap();

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        gm.kill_game(games.pop_front().unwrap()).await.unwrap();

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());

        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        gm.kill_game(games.pop_front().unwrap()).await.unwrap();

        handle.await;
    }

    std::thread::sleep(time::Duration::from_millis(100));

    Ok(())
}
