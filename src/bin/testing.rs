
extern crate mozaic;
extern crate tokio;
extern crate futures;

use mozaic::generic;
use mozaic::generic::*;

struct Foo {
    bar: usize,
}

impl generic::IDed for Foo {
    fn get_id() -> TypeID {
        0.into()
    }
}

struct Bar {
    foobar: u64,
}

impl generic::IDed for Bar {
    fn get_id() -> TypeID {
        1.into()
    }
}

fn main() {
    let mut reactor = generic::Reactor::new((), 0.into());

    reactor.add_handler(ReactorHandler::from(test_bar));
    reactor.add_handler(ReactorHandler::from(test_foo));

    tokio::run(
        futures::lazy(move || {
            let mut handle = reactor.get_handle();

            tokio::spawn(reactor);
            handle.send_internal(Bar { foobar: 2 });
            Ok(())
        }
    ));
}

fn test_foo(_state: &mut (), handle: &mut ReactorHandle<Message>, value: &Foo) {
    println!("foo: {}", value.bar);

    handle.send_internal(Bar { foobar: 2 });
}


fn test_bar(_state: &mut (), handle: &mut ReactorHandle<Message>, value: &Bar) {
    println!("foo: {}", value.foobar);

    handle.send_internal(Foo { bar: 0 });
}
