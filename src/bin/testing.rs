extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::any;
use std::env;

use mozaic::generic;
use mozaic::generic::*;

struct E(u64);

struct FooReactor(u64);

impl FooReactor {
    fn params(amount: u64) -> CoreParams<Self, any::TypeId, Message> {
        generic::CoreParams::new(FooReactor(amount))
    }
}

impl ReactorState<any::TypeId, Message> for FooReactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
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

fn main() {
    let args: Vec<String> = env::args().collect();
    let amount = args
        .get(1)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(10);

    let broker = BrokerHandle::new();
    let p1 = FooReactor::params(amount);
    let p2 = FooReactor::params(amount);

    tokio::run(futures::lazy(move || {
        broker.spawn(p2, Some(0.into()));
        broker.spawn(p1, Some(1.into()));

        Ok(())
    }));
}
