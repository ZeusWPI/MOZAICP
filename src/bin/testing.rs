#[macro_use]
extern crate futures;
extern crate mozaic;
extern crate mozaic_derive;
extern crate tokio;

use std::{any, env, time};

use mozaic::generic;
use mozaic::generic::*;

use futures::executor::{self, ThreadPool};

struct E(u64);

struct FooReactor(u64);
impl FooReactor {
    fn params(amount: u64) -> CoreParams<Self, any::TypeId, Message> {
        generic::CoreParams::new(FooReactor(amount))
    }
}

impl ReactorState<any::TypeId, Message> for FooReactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        println!("Here");

        let id: u64 = **handle.id();

        if id == 0 {
            handle.open_link(1.into(), FooLink::params(), true);
            handle.send_internal(E(self.0));
        } else {
            handle.open_link(0.into(), FooLink::params(), true);
        }
    }
}

struct FooLink();
impl FooLink {
    fn params() -> LinkParams<FooLink, any::TypeId, Message> {
        let mut params = LinkParams::new(FooLink());

        params.internal_handler(FunctionHandler::from(Self::handle_message));
        params.external_handler(FunctionHandler::from(Self::handle_message));

        return params;
    }

    fn handle_message(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &E) {
        let e = e.0 - 1;

        if e > 0 {
            handle.send_message(E(e));
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
