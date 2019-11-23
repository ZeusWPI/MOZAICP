
extern crate mozaic;
extern crate tokio;
extern crate futures;

use std::any;

use mozaic::generic;
use mozaic::generic::*;

struct E;
struct Init(u64, bool);

struct Foo {
    bar: u64,
}

impl Foo {
    fn params() -> LinkParams<Foo, any::TypeId, Message> {
        let mut params = LinkParams::new(Foo { bar: 5 });
        params.internal_handler(any::TypeId::of::<E>(), FunctionHandler::from(Self::test_bar));
        params.external_handler(any::TypeId::of::<E>(), FunctionHandler::from(Self::test_bar));

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

fn main() {
    let mut p1 = generic::CoreParams::new(());

    p1.handler(any::TypeId::of::<()>(), FunctionHandler::from(init));
    p1.handler(any::TypeId::of::<()>(), FunctionHandler::from(inner));

    let broker = BrokerHandle::new();


    let mut p2 = generic::CoreParams::new(());
    p2.handler(any::TypeId::of::<()>(), FunctionHandler::from(init));

    tokio::run(
        futures::lazy(move || {
            let id1 = broker.spawn(p1);
            let id2 = broker.spawn(p2);

            // h2.send_internal(Init(id1.into(), false));
            // h1.send_internal(Init(id2.into(), true));

            Ok(())
        }
    ));
}

fn init<'a>(_: &mut (), handle: &mut ReactorHandle<'a, any::TypeId, Message>, init: &Init) {
    println!("Init {}", init.0);
    handle.open_link(init.0.into(), Foo::params());

    if init.1 {
        println!("Sending internal");
        handle.send_internal(E);
    }
}

fn inner(_: &mut (), _: &mut ReactorHandle<'_, any::TypeId, Message>, _: &E) {
    println!("here");
}
