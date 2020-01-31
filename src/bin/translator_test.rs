#[macro_use]
extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::marker::PhantomData;
use std::{any, env, mem, time};

use mozaic::generic;
use mozaic::generic::*;

use futures::executor::{self, ThreadPool};

struct M1(Message);

impl Borrowable<M1> for M1 {
    fn borrow<'a, T: 'static + FromMessage<Msg = M1>>(&'a mut self) -> Option<&'a T> {
        self.0.borrow()
    }
}

impl Transmutable<any::TypeId> for M1 {
    fn transmute<T: 'static>(value: T) -> Option<(any::TypeId, Self)> {
        if let Some((t, m)) = Message::transmute(value) {
            Some((t, M1(m)))
        } else {
            None
        }
    }
}

struct M2(Message);

impl Borrowable<M2> for M2 {
    fn borrow<'a, T: 'static + FromMessage<Msg = M2>>(&'a mut self) -> Option<&'a T> {
        self.0.borrow()
    }
}

impl Transmutable<any::TypeId> for M2 {
    fn transmute<T: 'static>(value: T) -> Option<(any::TypeId, Self)> {
        if let Some((t, m)) = Message::transmute(value) {
            Some((t, M2(m)))
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct E1;
impl FromMessage for E1 {
    type Msg = M1;
}

#[derive(Clone)]
struct E2;
impl FromMessage for E2 {
    type Msg = M2;
}

struct FooLink<M: Send, E>(PhantomData<(M, E)>);
impl<
        M: 'static + Send + Transmutable<any::TypeId> + Borrowable<M>,
        E: 'static + Send + FromMessage<Msg = M> + Clone,
    > FooLink<M, E>
{
    fn params() -> LinkParams<FooLink<M, E>, any::TypeId, M> {
        let mut params = LinkParams::new(FooLink::<M, E>(PhantomData));

        params.internal_handler(FunctionHandler::from(Self::forword_message));
        params.external_handler(FunctionHandler::from(Self::backwards));

        return params;
    }

    fn forword_message(&mut self, handle: &mut LinkHandle<any::TypeId, M>, e: &E) {
        handle.send_message(e.clone());
    }

    fn backwards(&mut self, handle: &mut LinkHandle<any::TypeId, M>, e: &E) {
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

    fn thing(&mut self, handle: &mut ReactorHandle<any::TypeId, M1>, _: &E1) {
        self.0 -= 1;

        if self.0 <= 0 {
            handle.close();
        } else {
            handle.send_internal(E1);
        }
    }
}

impl ReactorState<any::TypeId, M1> for M1Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M1>) {
        // Setup translator
        let mut trans = Translator::<any::TypeId, any::TypeId, M1, M2>::new();
        trans.add_from(
            any::TypeId::of::<E2>(),
            Box::new(|_t, _m| M1::transmute(E1)),
        );
        trans.add_to(
            any::TypeId::of::<E1>(),
            Box::new(|_t, _m| M2::transmute(E2)),
        );

        let fn_to = trans.attach_to(11.into(), self.1.clone());
        let fn_from = trans.attach_from(*handle.id(), self.1.clone());

        let broker = BrokerHandle::new(self.1.clone());
        broker.spawn(M2Reactor::params(fn_from), Some(11.into()));

        handle.open_link(11.into(), FooLink::<M1, E1>::params(), true);
        handle.open_reactor_like(11.into(), fn_to(handle.chan()));

        handle.send_internal(E1);
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

    fn thing(&mut self, handle: &mut ReactorHandle<any::TypeId, M2>, _: &E2) {
        handle.send_internal(E2);
    }
}

impl ReactorState<any::TypeId, M2> for M2Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M2>) {
        let old = mem::replace(self, M2Reactor::Inited());
        if let M2Reactor::Init(spawn) = old {
            handle.open_link(10.into(), FooLink::<M2, E2>::params(), true);
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
