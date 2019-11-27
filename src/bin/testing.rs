extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::any;

use mozaic::generic;
use mozaic::generic::*;

struct E;

struct FooReactor();

impl FooReactor {
    fn params() -> CoreParams<Self, any::TypeId, Message> {
        let mut params = generic::CoreParams::new(FooReactor());

        params.handler(FunctionHandler::from(Self::handle_message));

        params
    }

    fn handle_message(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, _: &E) {
        println!("reactor internal message {:?}", handle.id());
    }
}

impl ReactorState<any::TypeId, Message> for FooReactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let id: u64 = **handle.id();
        println!("Init {}", id);

        if id == 0 {
            handle.open_link(1.into(), FooLink::params(5));
            handle.send_internal(E);
        } else {
            handle.open_link(0.into(), FooLink::params(5));
        }
    }
}

struct FooLink(u64);
impl FooLink {
    fn params(size: u64) -> LinkParams<FooLink, any::TypeId, Message> {
        let mut params = LinkParams::new(FooLink(size));

        params.internal_handler(FunctionHandler::from(Self::handle_message));
        params.external_handler(FunctionHandler::from(Self::handle_message));

        return params;
    }

    fn handle_message(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, _: &E) {
        self.0 -= 1;

        println!("At {}", self.0);

        if self.0 > 0 {
            handle.send_message(E);
        }
    }
}

fn main() {
    let p1 = FooReactor::params();

    let broker = BrokerHandle::new();

    let p2 = FooReactor::params();

    tokio::run(futures::lazy(move || {
        broker.spawn(p2, Some(0.into()));
        broker.spawn(p1, Some(1.into()));

        Ok(())
    }));
}
