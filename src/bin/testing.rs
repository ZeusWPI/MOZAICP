
extern crate mozaic;
extern crate tokio;
extern crate futures;

use std::any;

use mozaic::generic;
use mozaic::generic::*;

struct Foo {
    bar: u64,
}

struct Bar {
    foobar: u64,
}

fn main() {
    let mut params = generic::CoreParams::new(());

    params.handler(FunctionHandler::from(test_bar));
    params.handler(FunctionHandler::from(test_foo));

    let reactor = generic::Reactor::new(0.into(), BrokerHandle::new(), params);

    tokio::run(
        futures::lazy(move || {
            let mut handle = reactor.get_handle();

            tokio::spawn(reactor);
            handle.send_internal(Bar { foobar: 100000 });
            Ok(())
        }
    ));
}

fn test_foo(_state: &mut (), handle: &mut ReactorHandle<any::TypeId, Message>, value: &Foo) {
    // println!("foo: {}", value.bar);

    handle.send_internal(Bar { foobar: value.bar - 1 });
}

fn test_bar(_state: &mut (), handle: &mut ReactorHandle<any::TypeId, Message>, value: &Bar) {
    // println!("foo: {}", value.foobar);

    if value.foobar > 0 {
        handle.send_internal(Foo { bar: value.foobar });
    } else {
        handle.close();
    }
}
