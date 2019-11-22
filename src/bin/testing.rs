
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

fn main() {
    let mut p1 = generic::CoreParams::new(());

    p1.handler(FunctionHandler::from(init));
    p1.handler(FunctionHandler::from(inner));

    let broker = BrokerHandle::new();


    let mut p2 = generic::CoreParams::new(());
    p2.handler(FunctionHandler::from(init));

    tokio::run(
        futures::lazy(move || {
            let (id1, mut h1) = broker.spawn(p1);
            let (id2, mut h2) = broker.spawn(p2);

            h2.send_internal(Init(id1.into(), false));
            h1.send_internal(Init(id2.into(), true));

            Ok(())
        }
    ));
}

fn init(_: &mut (), handle: &mut ReactorHandle<any::TypeId, Message>, init: &Init) {
    println!("Init {}", init.0);
    let mut params = LinkParams::new(Foo { bar: 5 });
    params.internal_handler(FunctionHandler::from(test_bar));
    params.external_handler(FunctionHandler::from(test_bar));

    handle.open_link(init.0.into(), params);

    if init.1 {
        println!("Sending internal");
        handle.send_internal(());
    }
}

fn inner(_: &mut (), _: &mut ReactorHandle<any::TypeId, Message>, _: &()) {
    println!("here");
}

fn test_bar(state: &mut Foo, handle: &mut LinkHandle<any::TypeId, Message>, _: &()) {
    state.bar -= 1;
    println!("At {}", state.bar);

    if state.bar > 0 {
        println!("Sending message");
        handle.send_message(());
    }
}
