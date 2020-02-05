extern crate futures;
extern crate mozaic;
extern crate mozaic_derive;
#[macro_use]
extern crate tokio;
extern crate serde_json;
extern crate serde;

use std::{any, time};

use mozaic::generic;
use mozaic::generic::*;

use mozaic::modules::types::*;
use mozaic::modules::{ClientController, ConnectionManager};

use futures::executor::ThreadPool;

struct EchoReactor(Vec<ReactorID>);
impl EchoReactor {
    fn params(amount: Vec<ReactorID>) -> CoreParams<Self, any::TypeId, Message> {
        let mut params = generic::CoreParams::new(EchoReactor(amount));
        params.handler(FunctionHandler::from(Self::handle_msg));
        params
    }

    fn handle_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, e: &PlayerMsg) {
        println!("Echo ing");
        let value = format!("{}: {}\n", e.id, e.value);

        handle.send_internal(Data {
            value,
        }, TargetReactor::All);

        if "stop".eq_ignore_ascii_case(&e.value) {
            handle.close();
        }
    }
}

impl ReactorState<any::TypeId, Message> for EchoReactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        for cc in self.0.iter() {
            handle.open_link(*cc, EchoLink::params(), true);
        }
    }
}

struct EchoLink();
impl EchoLink {
    fn params() -> LinkParams<Self, any::TypeId, Message> {
        let mut params = LinkParams::new(Self());

        params.internal_handler(FunctionHandler::from(Self::handle_inc));
        params.external_handler(FunctionHandler::from(Self::handle_out));

        return params;
    }

    fn handle_inc(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &Data) {
        handle.send_message(e.clone());
    }

    fn handle_out(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &PlayerMsg) {
        handle.send_internal(e.clone(), TargetReactor::All);
    }
}

async fn run(pool: ThreadPool) {
    let broker = BrokerHandle::new(pool.clone());
    let json_broker = BrokerHandle::new(pool.clone());

    let echo_id = 0.into();
    let cm_id = 100.into();
    let ccs = vec![10, 11, 12];
    let p1 = EchoReactor::params(ccs.iter().map(|&x| x.into()).collect());

    let cm = ConnectionManager::params(
        pool.clone(),
        "127.0.0.1:6666".parse().unwrap(),
        json_broker.clone(),
        ccs.iter().map(|&x| (x+1, x.into())).collect(),
    );

    ccs.iter().map(
        |&id| ClientController::new(
            id.into(),
            json_broker.clone(),
            broker.clone(),
            echo_id,
            cm_id,
            id + 1
        )
    ).for_each(|cc| pool.spawn_ok(cc));

    join!(
        broker.spawn_with_handle(p1, Some(echo_id)).0,
        json_broker.spawn_with_handle(cm, Some(cm_id)).0,
    );
}

#[tokio::main]
async fn main() {
    {
        let pool = ThreadPool::builder()
            // .after_start(|i| println!("Starting thread {}", i))
            // .before_stop(|i| println!("Stopping thread {}", i))
            .create()
            .unwrap();

        run(pool).await;
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
