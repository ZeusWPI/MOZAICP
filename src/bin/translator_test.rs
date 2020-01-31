#[macro_use]
extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::{any, env, time, mem};
use std::marker::PhantomData;

use mozaic::generic;
use mozaic::generic::*;

use futures::executor::{self, ThreadPool};

struct M1(Message);

impl Borrowable for M1 {
    fn borrow<'a, T: 'static>(&'a mut self) -> Option<&'a T> {
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

impl Borrowable for M2 {
    fn borrow<'a, T: 'static>(&'a mut self) -> Option<&'a T> {
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

struct E;
struct FooLink<M: Send>(PhantomData<M>);
impl<M: 'static + Send + Transmutable<any::TypeId> + Borrowable> FooLink<M> {
    fn params() -> LinkParams<FooLink<M>, any::TypeId, M> {
        let mut params = LinkParams::new(FooLink::<M>(PhantomData));

        params.internal_handler(FunctionHandler::from(Self::forword_message));
        params.external_handler(FunctionHandler::from(Self::backwards));

        return params;
    }

    fn forword_message(&mut self, handle: &mut LinkHandle<any::TypeId, M>, _: &E) {
        handle.send_message(E);
    }

    fn backwards(&mut self, handle: &mut LinkHandle<any::TypeId, M>, _: &E) {
        handle.send_internal(E);
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
            handle.send_internal(E);
        }
    }
}

impl ReactorState<any::TypeId, M1> for M1Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M1>) {
        // Setup translator
        let mut trans = Translator::<any::TypeId, any::TypeId, M1, M2>::new();
        trans.add_from(any::TypeId::of::<E>(), Box::new(|_t, _m| {
            M1::transmute(E)
        }));
        trans.add_to(any::TypeId::of::<E>(), Box::new(|_t, _m| {
            M2::transmute(E)
        }));

        let fn_to = trans.attach_to(11.into(), self.1.clone());
        let fn_from = trans.attach_from(*handle.id(), self.1.clone());

        let broker = BrokerHandle::new(self.1.clone());
        broker.spawn(M2Reactor::params(fn_from), Some(11.into()));

        handle.open_link(11.into(), FooLink::params(), true);
        handle.open_reactor_like(11.into(), fn_to(handle.chan()));

        handle.send_internal(E);
    }
}

enum M2Reactor {
    Init(Box<dyn FnOnce(Sender<any::TypeId, M2>) -> Sender<any::TypeId, M2> + Send>),
    Inited(),
}

impl M2Reactor {
    fn params(s: Box<dyn FnOnce(Sender<any::TypeId, M2>) -> Sender<any::TypeId, M2> + Send>) -> CoreParams<Self, any::TypeId, M2> {
        let mut params = generic::CoreParams::new(M2Reactor::Init(s));
        params.handler(FunctionHandler::from(Self::thing));
        params
    }

    fn thing(&mut self, handle: &mut ReactorHandle<any::TypeId, M2>, _: &E) {
        handle.send_internal(E);
    }
}


impl ReactorState<any::TypeId, M2> for M2Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M2>) {

        let old = mem::replace(self, M2Reactor::Inited());
        if let M2Reactor::Init(spawn) = old {
            handle.open_link(10.into(), FooLink::params(), true);
            handle.open_reactor_like(10.into(), spawn(handle.chan()));
        }
    }
}

async fn run(amount: u64, pool: ThreadPool) {
    let broker = BrokerHandle::new(pool.clone());
    let p1 = M1Reactor::params(amount, pool.clone());

    join!(
        broker.spawn_with_handle(p1, Some(10.into())).0,
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let amount = args
        .get(1)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(10);

    {
        let pool = ThreadPool::builder()
            .pool_size(2)
            .create()
            .unwrap();

        executor::block_on(run(amount, pool));
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
