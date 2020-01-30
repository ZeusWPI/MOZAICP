#[macro_use]
extern crate futures;
extern crate mozaic;
extern crate tokio;

use std::{any, env, time, mem};

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

use std::marker::PhantomData;

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
        println!("At {}", self.0);
        
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
        println!("M1Reactor");

        // Setup translator
        let mut trans = Translator::<any::TypeId, any::TypeId, M1, M2>::new(11.into(), 10.into());
        trans.add_from(any::TypeId::of::<E>(), Box::new(|_t, _m| {
            // let mo = m.borrow::<E>().unwrap();
            M1::transmute(E).unwrap()
        }));

        trans.add_to(any::TypeId::of::<E>(), Box::new(|_t, _m| {
            // let mo = m.borrow::<E>().unwrap();
            M2::transmute(E).unwrap()
        }));

        let (s1, l1) = trans.attach_to(self.1.clone());
        let (s2, l2) = trans.attach_from(self.1.clone());

        let broker = BrokerHandle::new(self.1.clone());
        broker.spawn(M2Reactor::params(s2, l2), Some(0.into()));

        handle.open_link_like(11.into(), l1, true, s1);
        handle.open_link(11.into(), FooLink::params(), true);
        handle.send_internal(E);
    }
}

enum M2Reactor {
    Init(Sender<any::TypeId, M2>, LinkSpawner<any::TypeId, M2>),
    Inited(),
}

impl M2Reactor {
    fn params(s: Sender<any::TypeId, M2>, l: LinkSpawner<any::TypeId, M2>) -> CoreParams<Self, any::TypeId, M2> {
        let mut params = generic::CoreParams::new(M2Reactor::Init(s, l));
        params.handler(FunctionHandler::from(Self::thing));
        params
    }

    fn thing(&mut self, handle: &mut ReactorHandle<any::TypeId, M2>, _: &E) {
        handle.send_internal(E);
    }
}


impl ReactorState<any::TypeId, M2> for M2Reactor {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, M2>) {
        println!("M2 reactor");

        let old = mem::replace(self, M2Reactor::Inited());
        if let M2Reactor::Init(sender, spawner) = old {
            handle.open_link_like(10.into(), spawner, true, sender);
            handle.open_link(10.into(), FooLink::params(), true);
        }

    }
}

async fn run(amount: u64, pool: ThreadPool) {
    let broker = BrokerHandle::new(pool.clone());
    let p1 = M1Reactor::params(amount, pool.clone());

    join!(
        broker.spawn_with_handle(p1, Some(0.into())).0,
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
            // .after_start(|i| println!("Starting thread {}", i))
            // .before_stop(|i| println!("Stopping thread {}", i))
            .create()
            .unwrap();

        executor::block_on(run(amount, pool));
    }

    std::thread::sleep(time::Duration::from_millis(100));
}
