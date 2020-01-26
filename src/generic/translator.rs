use futures::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use super::*;
use std::collections::HashMap;
use std::hash::Hash;

enum Combined<K1, K2, M1, M2> {
    To(K1, M1),
    From(K2, M2),
}

pub struct Translator<K1, K2, M1, M2> {
    map_to: HashMap<K1, Box<dyn Fn(K1, M1) -> (K2, M2) + Send>>,
    map_from: HashMap<K2, Box<dyn Fn(K2, M2) -> (K1, M1) + Send>>,

    to_chan: (Sender<K1, M1>, Receiver<K1, M1>),
    from_chan: (Sender<K2, M2>, Receiver<K2, M2>),
    comb_chan: (
        mpsc::UnboundedSender<Combined<K1, K2, M1, M2>>,
        mpsc::UnboundedReceiver<Combined<K1, K2, M1, M2>>,
    ),
}

impl<K1, K2, M1, M2> Translator<K1, K2, M1, M2>
where
    K1: Hash + Eq + Send + 'static,
    K2: Hash + Eq + Send + 'static,
    M1: Send + 'static,
    M2: Send + 'static,
{
    pub fn new() -> Self {
        Self {
            map_to: HashMap::new(),
            map_from: HashMap::new(),
            to_chan: mpsc::unbounded(),
            from_chan: mpsc::unbounded(),
            comb_chan: mpsc::unbounded(),
        }
    }

    pub fn add_to(&mut self, key: K1, to_f: Box<dyn Fn(K1, M1) -> (K2, M2) + Send>) {
        self.map_to.insert(key, to_f);
    }

    pub fn add_from(&mut self, key: K2, from_f: Box<dyn Fn(K2, M2) -> (K1, M1) + Send>) {
        self.map_from.insert(key, from_f);
    }

    pub fn attach_to(&mut self) -> (Sender<K1, M1>, LinkSpawner<K1, M1>) {
        (
            self.to_chan.0.clone(),
            Box::new(move |(_, target, _, _)| {
                Box::new(Helper::<K1, M1, K2, M2> { target, map: HashMap::new() })
            })
        )
    }
}

struct Helper<K1, M1, K2, M2> {
    target: Sender<K1, M1>,
    map: HashMap<K1, Box<dyn Fn(K1, M1) -> (K2, M2) + Send>>,
}

impl<'a, 'b, K, M, K2, M2> Handler<(), ReactorHandle<'b, K, M>, LinkOperation<'a, K, M>>
    for Helper<K, M, K2, M2>
where
    K: Hash + Eq,
{
    fn handle(
        &mut self,
        _: &mut (),
        _handle: &mut ReactorHandle<'b, K, M>,
        m: &mut LinkOperation<K, M>,
    ) {
        match m {
            LinkOperation::ExternalMessage(key, message) => {},
            _ => unimplemented!(),
        }
    }
}
