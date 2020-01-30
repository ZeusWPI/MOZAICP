#[macro_use]
extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::{env, time, process};

use mozaic::core_capnp::{identify, initialize};
use mozaic::errors::*;
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::runtime::Broker;

use futures::executor::ThreadPool;

struct FooReactor(u64);

impl FooReactor {
    fn params<C: Ctx>(amount: u64) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(FooReactor(amount));

        params.handler(initialize::Owned, CtxHandler::new(Self::initialize));

        params
    }

    // reactor setup
    fn initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()> {
        println!("Here");

        let id = handle.id().clone();

        if id == 0.into() {
            handle.open_link(FooLink::params(1.into()))?;
            let mut joined = MsgBuffer::<identify::Owned>::new();
            joined.build(|b| {
                b.set_key(self.0);
            });
            handle.send_internal(joined)?;
        } else {
            handle.open_link(FooLink::params(0.into()))?;
        }

        return Ok(());
    }
}

struct FooLink();
impl FooLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, FooLink());

        params.external_handler(identify::Owned, CtxHandler::new(Self::handle_message));

        params.internal_handler(identify::Owned, CtxHandler::new(Self::handle_message));

        return params;
    }

    fn handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        e: identify::Reader,
    ) -> Result<()> {
        let e = e.get_key() - 1;

        if e > 0 {
            let mut joined = MsgBuffer::<identify::Owned>::new();
            joined.build(|b| {
                b.set_key(e);
            });
            handle.send_message(joined)?;
        } else {
            println!("Done");
            process::exit(0);
        }

        Ok(())
    }
}

async fn run(amount: u64, pool: ThreadPool) {
    let mut broker = Broker::new(pool.clone()).unwrap();
    let p1 = FooReactor::params(amount);
    let p2 = FooReactor::params(amount);

    join!(
        broker.spawn_with_handle(0.into(), p1, "main").unwrap(),
        broker.spawn_with_handle(1.into(), p2, "main").unwrap()
    );
}

use futures::executor;

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
