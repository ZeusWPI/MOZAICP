#[macro_use]
extern crate futures;
extern crate mozaic;
#[macro_use]
extern crate mozaic_derive;
extern crate tokio;
#[macro_use] extern crate serde;

use std::{any, env, time};

use mozaic::generic;
use mozaic::generic::*;
use mozaic::modules::net;

use futures::executor::{self, ThreadPool};

#[derive(Serialize, Deserialize, Key)]
struct E {
    value: u64,
    type_id: String,
}

impl E {
    fn new(value: u64) -> Self {
        Self {
            value,
            type_id: Self::key(),
        }
    }
}

struct FooReactor(u64);
impl FooReactor {
    fn params(amount: u64) -> CoreParams<Self, String, JSONMessage> {
        generic::CoreParams::new(FooReactor(amount))
    }
}

impl ReactorState<String, JSONMessage> for FooReactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, String, JSONMessage>) {
        let id: u64 = **handle.id();

        if id == 0 {
            handle.open_link(1.into(), FooLink::params(), true);
            handle.send_internal(E::new(self.0));
        } else {
            handle.open_link(0.into(), FooLink::params(), true);
        }
    }
}

struct FooLink();
impl FooLink {
    fn params() -> LinkParams<FooLink, String, JSONMessage> {
        let mut params = LinkParams::new(FooLink());

        params.internal_handler(FunctionHandler::from(Self::handle_message));
        params.external_handler(FunctionHandler::from(Self::handle_message));

        return params;
    }

    fn handle_message(&mut self, handle: &mut LinkHandle<String, JSONMessage>, e: &E) {
        let e = e.value - 1;

        if e > 0 {
            handle.send_message(E::new(e));
        } else {
            println!("Done {:?} -> {:?}", handle.source_id(), handle.target_id());
            handle.close_link();
        }
    }
}

async fn run(amount: u64, pool: ThreadPool) {
    let broker = BrokerHandle::new(pool);
    let p1 = FooReactor::params(amount);
    let p2 = FooReactor::params(amount);

    join!(
        broker.spawn_with_handle(p2, Some(0.into())).0,
        broker.spawn_with_handle(p1, Some(1.into())).0,
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let amount = args
        .get(1)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(10);

    {
        let pool = ThreadPool::builder()
            // .after_start(|i| println!("Starting thread {}", i))
            // .before_stop(|i| println!("Stopping thread {}", i))
            .create()
            .unwrap();

        executor::block_on(run(amount, pool));
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
