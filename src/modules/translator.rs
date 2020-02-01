use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::{future, FutureExt};

use std::collections::HashMap;
use std::hash::Hash;

use crate::generic::*;

enum InnerMsg<K, M> {
    Close(),
    Msg(K, M),
}

pub struct Translator<K1, K2, M1, M2> {
    to: Option<(
        mpsc::UnboundedReceiver<InnerMsg<K1, M1>>,
        HashMap<K1, Box<dyn Fn(&K1, &M1) -> Option<(K2, M2)> + Send + Sync>>,
    )>,
    from: Option<(
        mpsc::UnboundedReceiver<InnerMsg<K2, M2>>,
        HashMap<K2, Box<dyn Fn(&K2, &M2) -> Option<(K1, M1)> + Send + Sync>>,
    )>,

    s1: mpsc::UnboundedSender<InnerMsg<K1, M1>>,
    s2: mpsc::UnboundedSender<InnerMsg<K2, M2>>,
}

impl<K1, K2, M1, M2> Translator<K1, K2, M1, M2>
where
    K1: Hash + Eq + Send + Sync + 'static,
    K2: Hash + Eq + Send + Sync + 'static,
    M1: Send + Sync + 'static,
    M2: Send + Sync + 'static,
{
    pub fn new() -> Self {
        let (s1, r1) = mpsc::unbounded();
        let (s2, r2) = mpsc::unbounded();

        Self {
            s1: s1,
            s2: s2,
            to: Some((r1, HashMap::new())),
            from: Some((r2, HashMap::new())),
        }
    }

    pub fn add_to(
        &mut self,
        key: K1,
        to_f: Box<dyn Fn(&K1, &M1) -> Option<(K2, M2)> + Send + Sync>,
    ) {
        self.to.as_mut().map(|to| to.1.insert(key, to_f));
    }

    pub fn add_from(
        &mut self,
        key: K2,
        from_f: Box<dyn Fn(&K2, &M2) -> Option<(K1, M1)> + Send + Sync>,
    ) {
        self.from.as_mut().map(|from| from.1.insert(key, from_f));
    }

    fn attach(
        origin: ReactorID,
        pool: ThreadPool,
        rec: mpsc::UnboundedReceiver<InnerMsg<K1, M1>>,
        s2: mpsc::UnboundedSender<InnerMsg<K2, M2>>,
        map: HashMap<K1, Box<dyn Fn(&K1, &M1) -> Option<(K2, M2)> + Send + Sync>>,
    ) -> Box<dyn FnOnce(Sender<K1, M1>) -> Sender<K1, M1> + Send> {
        Box::new(move |sender| {
            let (tx, rx) = mpsc::unbounded();

            // translate msg from rx to something and send it to s2
            pool.spawn_ok(
                rx.filter_map(move |item| {
                    future::ready(match item {
                        Operation::ExternalMessage(_, k, m) => {
                            map.get(&k).unwrap()(&k, &m).map(|(k, m)| InnerMsg::Msg(k, m))
                        }
                        Operation::CloseLink(_) => Some(InnerMsg::Close()),
                        _ => unimplemented!(),
                    })
                })
                .for_each(move |item| {
                    s2.unbounded_send(item).expect("Send failed");
                    future::ready(())
                })
                .boxed(),
            );

            // pass msg through from rec to sender
            pool.spawn_ok(
                rec.map(move |item| match item {
                    InnerMsg::Close() => Operation::CloseLink(origin),
                    InnerMsg::Msg(k, m) => Operation::ExternalMessage(origin, k, m),
                })
                .for_each(move |item| {
                    if sender.unbounded_send(item).is_err() {
                        println!("Translator channel closed");
                    }
                    future::ready(())
                })
                .boxed(),
            );

            tx
        })
    }

    pub fn attach_to(
        &mut self,
        origin: ReactorID,
        pool: ThreadPool,
    ) -> Box<dyn FnOnce(Sender<K1, M1>) -> Sender<K1, M1> + Send> {
        let (rec, map) = std::mem::replace(&mut self.to, None).unwrap();
        let s2 = self.s2.clone();

        Translator::attach(origin, pool, rec, s2, map)
    }

    pub fn attach_from(&mut self, origin: ReactorID, pool: ThreadPool
    ) -> Box<dyn FnOnce(Sender<K2, M2>) -> Sender<K2, M2> + Send> {
        let (rec, map) = std::mem::replace(&mut self.from, None).unwrap();
        let s1 = self.s1.clone();

        Translator::attach(origin, pool, rec, s1, map)
    }
}
