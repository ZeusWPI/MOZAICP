
extern crate mozaic;
extern crate tokio;
extern crate futures;

use std::any;

use mozaic::generic;
use mozaic::generic::*;

struct E;

struct Foo {
    bar: u64,
}

impl Foo {
    fn params() -> LinkParams<Foo, any::TypeId, Message> {
        let mut params = LinkParams::new(Foo { bar: 5 });

        params.internal_handler(FunctionHandler::from(Self::test_bar));
        params.external_handler(FunctionHandler::from(Self::test_bar));

        params
    }

    fn test_bar(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, _: &E) {
        self.bar -= 1;
        println!("At {}", self.bar);

        if self.bar > 0 {
            println!("Sending message");
            handle.send_message(E);
        }
    }
}

impl ReactorState<any::TypeId, Message> for Foo {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message> ) {
        let id: u64 = **handle.id();
        println!("Init {}", id);

        if id == 0 {
            handle.open_link(1.into(), Foo::params());
        handle.send_internal(E);

        } else {
            println!("Open 1");
            handle.open_link(0.into(), Foo::params());
            println!("Sending");
            handle.send_internal(E);
        }
    }
}

fn main() {
    let mut p1 = generic::CoreParams::new(Foo { bar: 0 });
    p1.handler(FunctionHandler::from(inner));

    let broker = BrokerHandle::new();

    let p2 = generic::CoreParams::new(Foo { bar: 0 });

    tokio::run(
        futures::lazy(move || {
            broker.spawn(p2, Some(0.into()));

            println!("HEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE");
            broker.spawn(p1, Some(1.into()));

            Ok(())
        }
    ));
}

fn inner(_: &mut Foo, _: &mut ReactorHandle<'_, any::TypeId, Message>, _: &E) {
    println!("here");
}
