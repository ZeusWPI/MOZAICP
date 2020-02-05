#[macro_use]
extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::{any, env, mem, time};

use mozaic::generic;
use mozaic::generic::*;

use mozaic::modules::Translator;

use futures::executor::{self, ThreadPool};

struct M1(Message);
impl FromMessage<any::TypeId, M1> for E {
    fn from_msg<'a>(_: &any::TypeId, msg: &'a mut M1) -> Option<&'a E> {
        msg.0.borrow()
    }
}
impl IntoMessage<any::TypeId, M1> for E {
    fn into_msg(self) -> Option<(any::TypeId, M1)> {
        <E as IntoMessage<any::TypeId, Message>>::into_msg(self).map(|(k, m)| (k, M1(m)))
    }
}

struct M2(Message);
impl FromMessage<any::TypeId, M2> for E {
    fn from_msg<'a>(_: &any::TypeId, msg: &'a mut M2) -> Option<&'a E> {
        msg.0.borrow()
    }
}
impl IntoMessage<any::TypeId, M2> for E {
    fn into_msg(self) -> Option<(any::TypeId, M2)> {
        <E as IntoMessage<any::TypeId, Message>>::into_msg(self).map(|(k, m)| (k, M2(m)))
    }
}

#[derive(Clone)]
struct E;

struct M1FooLink();
impl M1FooLink {
    fn params() -> LinkParams<M1FooLink, any::TypeId, M1> {
        let mut params = LinkParams::new(M1FooLink());

        params.internal_handler(FunctionHandler::from(Self::forword_message));
        params.external_handler(FunctionHandler::from(Self::backwards));

        return params;
    }

    fn forword_message(&mut self, handle: &mut LinkHandle<any::TypeId, M1>, e: &E) {
        handle.send_message(e.clone());
    }

    fn backwards(&mut self, handle: &mut LinkHandle<any::TypeId, M1>, e: &E) {
        handle.send_internal(e.clone());
    }
}

struct M2FooLink();
impl M2FooLink {
    fn params() -> LinkParams<M2FooLink, any::TypeId, M2> {
        let mut params = LinkParams::new(M2FooLink());

        params.internal_handler(FunctionHandler::from(Self::forword_message));
        params.external_handler(FunctionHandler::from(Self::backwards));

        return params;
    }

    fn forword_message(&mut self, handle: &mut LinkHandle<any::TypeId, M2>, e: &E) {
        handle.send_message(e.clone());
    }

    fn backwards(&mut self, handle: &mut LinkHandle<any::TypeId, M2>, e: &E) {
        handle.send_internal(e.clone());
    }
}

struct M1Reactor(u64, ThreadPool);
impl M1Reactor {
    fn params(amount: u64, pool: ThreadPool) -> CoreParams<Self, any::TypeId, M1> {
        let mut params = generic::CoreParams::new(Self(amount, pool));
        params.handler(FunctionHandler::from(Self::thing));
        params
    }

    fn thing(&mut self, handle: &mut ReactorHandle<any::TypeId, M1>, _: &E) {
        self.0 -= 1;

        if self.0 <= 0 {
            handle.close();
        } else {
            handle.send_internal(E, TargetReactor::Links);
        }
    }
}

impl ReactorState<any::TypeId, M1> for M1Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M1>) {
        // Setup translator
        let mut trans = Translator::<any::TypeId, any::TypeId, M1, M2>::new();
        trans.add_from(any::TypeId::of::<E>(), Box::new(|_t, _m| E::into_msg(E)));
        trans.add_to(any::TypeId::of::<E>(), Box::new(|_t, _m| E::into_msg(E)));

        let fn_to = trans.attach_to(11.into(), self.1.clone());
        let fn_from = trans.attach_from(*handle.id(), self.1.clone());

        let broker = BrokerHandle::new(self.1.clone());
        broker.spawn(M2Reactor::params(fn_from), Some(11.into()));

        handle.open_link(11.into(), M1FooLink::params(), true);
        handle.open_reactor_like(11.into(), fn_to(handle.chan()));

        handle.send_internal(E, TargetReactor::Links);
    }
}

enum M2Reactor {
    Init(Box<dyn FnOnce(Sender<any::TypeId, M2>) -> Sender<any::TypeId, M2> + Send>),
    Inited(),
}

impl M2Reactor {
    fn params(
        s: Box<dyn FnOnce(Sender<any::TypeId, M2>) -> Sender<any::TypeId, M2> + Send>,
    ) -> CoreParams<Self, any::TypeId, M2> {
        let mut params = generic::CoreParams::new(M2Reactor::Init(s));
        params.handler(FunctionHandler::from(Self::thing));
        params
    }

    fn thing(&mut self, handle: &mut ReactorHandle<any::TypeId, M2>, _: &E) {
        handle.send_internal(E, TargetReactor::Links);
    }
}

impl ReactorState<any::TypeId, M2> for M2Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M2>) {
        let old = mem::replace(self, M2Reactor::Inited());
        if let M2Reactor::Init(spawn) = old {
            handle.open_link(10.into(), M2FooLink::params(), true);
            handle.open_reactor_like(10.into(), spawn(handle.chan()));
        }
    }
}

async fn run(amount: u64, pool: ThreadPool) {
    let broker = BrokerHandle::new(pool.clone());
    let p1 = M1Reactor::params(amount, pool.clone());

    join!(broker.spawn_with_handle(p1, Some(10.into())).0,);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let amount = args
        .get(1)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(10);

    {
        let pool = ThreadPool::builder().pool_size(2).create().unwrap();

        executor::block_on(run(amount, pool));
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
