extern crate futures;
extern crate mozaic;
extern crate mozaic_derive;
#[macro_use]
extern crate tokio;
extern crate serde;
extern crate serde_json;

extern crate tracing;
extern crate tracing_subscriber;

use tracing_subscriber::{FmtSubscriber, EnvFilter};

use std::{any, time};

use mozaic::generic;
use mozaic::generic::*;

use mozaic::modules::types::*;
use mozaic::modules::{ClientController, ConnectionManager, Aggregator, StepLock};

use futures::executor::ThreadPool;

struct EchoReactor(ReactorID);
impl EchoReactor {
    fn params(clients: ReactorID) -> CoreParams<Self, any::TypeId, Message> {
        generic::CoreParams::new(EchoReactor(clients))
            .handler(FunctionHandler::from(Self::handle_msg))
    }

    fn handle_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, e: &PlayerMsg) {
        println!("Echo ing");
        let value = format!("{}: {}\n", e.id, e.value);

        handle.send_internal(HostMsg { value, target: None }, TargetReactor::All);

        if "stop".eq_ignore_ascii_case(&e.value) {
            handle.close();
        }

        // ?: Add way of player to quit the game, echo server whatever
        // if "quit".eq_ignore_ascii_case(&e.value) {
        //     handle.send_internal(Typed::from(Close{}, TargetReactor::Link(e.))
        // }
    }
}

impl ReactorState<any::TypeId, Message> for EchoReactor {
    const NAME: &'static str = "EchoReactor";
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        handle.open_link(self.0, EchoLink::params(), true);
    }
}

struct EchoLink();
impl EchoLink {
    fn params() -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self())
            .internal_handler(FunctionHandler::from(Self::handle_inc))
            .external_handler(FunctionHandler::from(Self::handle_out))
    }

    fn handle_inc(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &HostMsg) {
        handle.send_message(e.clone());
    }

    fn handle_out(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &PlayerMsg) {
        handle.send_internal(e.clone(), TargetReactor::Reactor);
    }
}

async fn run(pool: ThreadPool) {
    use std::collections::HashMap;

    let broker = BrokerHandle::new(pool.clone());
    let json_broker = BrokerHandle::new(pool.clone());

    let player_ids = vec![10, 11];
    let cc_ids = vec![11.into(), 12.into()];

    let player_map: HashMap<PlayerId, ReactorID> = player_ids.iter().cloned().zip(cc_ids.iter().cloned()).collect();

    let echo_id = 0.into();
    let step_id = 2.into();
    let agg_id = 1.into();
    let cm_id = 100.into();

    let echo = EchoReactor::params(step_id);
    let agg = Aggregator::params(step_id, player_map.clone());
    let step = StepLock::params(echo_id, agg_id, player_ids, Some(3000));

    let cm = ConnectionManager::params(
        pool.clone(),
        "127.0.0.1:6666".parse().unwrap(),
        json_broker.clone(),
        player_map.clone(),
    );

    player_map.iter()
        .map(|(&id, &r_id)| {
            ClientController::new(
                r_id,
                json_broker.clone(),
                broker.clone(),
                agg_id,
                cm_id,
                id,
            )
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
