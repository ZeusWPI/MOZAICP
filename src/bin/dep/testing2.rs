extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::env;
use std::process;

use mozaic::core_capnp::{identify, initialize};
use mozaic::errors::*;
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::runtime::Broker;

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

fn main() {
    let args: Vec<String> = env::args().collect();
    let amount = args
        .get(1)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(10);

    let mut broker = Broker::new().unwrap();
    let p1 = FooReactor::params(amount);
    let p2 = FooReactor::params(amount);

    tokio::run(futures::lazy(move || {
        broker.spawn(0.into(), p1, "main").display();

        broker.spawn(1.into(), p2, "main").display();

        Ok(())
    }));
}
