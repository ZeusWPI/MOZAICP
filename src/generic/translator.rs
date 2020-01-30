use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::{Future, Stream};

use super::*;
use std::collections::HashMap;
use std::hash::Hash;

enum Combined<K1, K2, M1, M2> {
    To(K1, M1),
    From(K2, M2),
}

pub struct Translator<K1, K2, M1, M2> {
    to: Option<(
        Receiver<K1, M1>,
        HashMap<K1, Box<dyn Fn(&K1, &M1) -> (K2, M2) + Send>>,
    )>,
    from: Option<(
        Receiver<K2, M2>,
        HashMap<K2, Box<dyn Fn(&K2, &M2) -> (K1, M1) + Send>>,
    )>,

    s1: Sender<K1, M1>,
    s2: Sender<K2, M2>,
    id1: ReactorID,
    id2: ReactorID,
}

impl<K1, K2, M1, M2> Translator<K1, K2, M1, M2>
where
    K1: Hash + Eq + Send + 'static,
    K2: Hash + Eq + Send + 'static,
    M1: Send + 'static,
    M2: Send + 'static,
{
    pub fn new(id1: ReactorID, id2: ReactorID) -> Self {
        let (s1, r1) = mpsc::unbounded();
        let (s2, r2) = mpsc::unbounded();
        Self {
            s1: s1,
            s2: s2,
            to: Some((r1, HashMap::new())),
            from: Some((r2, HashMap::new())),
            id1, id2,
        }
    }

    pub fn add_to(&mut self, key: K1, to_f: Box<dyn Fn(&K1, &M1) -> (K2, M2) + Send>) {
        self.to.as_mut().map(|to| to.1.insert(key, to_f));
    }

    pub fn add_from(&mut self, key: K2, from_f: Box<dyn Fn(&K2, &M2) -> (K1, M1) + Send>) {
        self.from.as_mut().map(|from| from.1.insert(key, from_f));
    }

    pub fn attach_to(&mut self, pool: ThreadPool) -> (Sender<K1, M1>, LinkSpawner<K1, M1>) {
        let (rec, map) = std::mem::replace(&mut self.to, None).unwrap();

        let sender = self.s2.clone();
        let id = self.id2.clone();
        (
            self.s1.clone(), // This shouldn't be used
            Box::new(move |(target, _, _, _)| {
                pool.spawn_ok(OtherHelper {
                    sender: target,
                    receiver: rec,
                });

                Box::new(Helper::<K1, M1, K2, M2> { sender, map, id })
            }),
        )
    }

    pub fn attach_from(&mut self, pool: ThreadPool) -> (Sender<K2, M2>, LinkSpawner<K2, M2>) {
        let (rec, map) = std::mem::replace(&mut self.from, None).unwrap();

        let sender = self.s1.clone();
        let id = self.id1.clone();
        (
            self.s2.clone(),
            Box::new(move |(target, _, _, _)| {
                pool.spawn_ok(OtherHelper {
                    sender: target,
                    receiver: rec,
                });

                Box::new(Helper::<K2, M2, K1, M1> { sender, map, id })
            }),
        )
    }
}

struct Helper<K1, M1, K2, M2> {
    sender: Sender<K2, M2>,
    id: ReactorID,
    map: HashMap<K1, Box<dyn Fn(&K1, &M1) -> (K2, M2) + Send>>,
}

impl<'a, 'b, K1, M1, K2, M2> Handler<(), ReactorHandle<'b, K1, M1>, LinkOperation<'a, K1, M1>>
    for Helper<K1, M1, K2, M2>
where
    K1: Hash + Eq,
{
    fn handle(
        &mut self,
        _: &mut (),
        _handle: &mut ReactorHandle<'b, K1, M1>,
        m: &mut LinkOperation<K1, M1>,
    ) {
        match m {
            LinkOperation::ExternalMessage(key, message) => {
                if let Some(translator) = self.map.get(&key) {
                    let (k, m) = translator(key, message);
                    println!("Sending");
                    self.sender
                        .unbounded_send(Operation::ExternalMessage(0.into(), k, m))
                        .expect("Soemthing happend");
                }
            }
            LinkOperation::InternalMessage(key, message) => {
                if let Some(translator) = self.map.get(&key) {
                    let (k, m) = translator(key, message);
                    println!("Sending");
                    self.sender
                        .unbounded_send(Operation::ExternalMessage(0.into(), k, m))
                        .expect("Soemthing happend");
                }
            }
            LinkOperation::Close() => {
                self.sender.unbounded_send(Operation::CloseLink(self.id)).expect("Bla bla here");
            }
        }
    }
}

struct OtherHelper<K, M> {
    sender: Sender<K, M>,
    receiver: Receiver<K, M>,
}

use futures::task::{Context, Poll};
use std::pin::Pin;

impl<K, M> Future for OtherHelper<K, M> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);
        loop {
            match ready!(Stream::poll_next(Pin::new(&mut this.receiver), ctx)) {
                None => return Poll::Ready(()),
                Some(item) => {
                    println!("Copying something");
                    if this.sender.unbounded_send(item).is_err() {
                        println!("Couldn't close translator channel");
                    }
                }
            }
        }
    }
}
