use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::{future, FutureExt};

use std::collections::HashMap;
use std::hash::Hash;

use crate::generic::*;

use std::marker::PhantomData;
struct Helper<T, K1, M1, K2, M2> {
    pd: PhantomData<(T, K2, M2)>,
    f: Box<dyn Fn(&K1, &M1) -> Option<T> + Send + Sync + 'static>,
}

impl<T, K1, M1, K2, M2> Helper<T, K1, M1, K2, M2> {
    fn new<F>(f: F) -> Self
    where
        F: Fn(&K1, &M1) -> Option<T> + 'static + Send + Sync,
    {
        Self {
            f: Box::new(f),
            pd: PhantomData,
        }
    }
}

trait HelperHandler<K1, M1, K2, M2> {
    fn handle(
        &self,
        origin: ReactorID,
        k: &K1,
        m: &M1,
        handler: &SenderHandle<K2, M2>,
    ) -> Option<()>;
}

impl<T, K1, M1, K2, M2> HelperHandler<K1, M1, K2, M2> for Helper<T, K1, M1, K2, M2>
where
    K1: Hash + Eq + Send + Sync + 'static + Unpin,
    K2: Hash + Eq + Send + Sync + 'static + Unpin,
    M1: Send + Sync + 'static,
    M2: Send + Sync + 'static,
    T: IntoMessage<K2, M2>,
{
    fn handle(
        &self,
        origin: ReactorID,
        k: &K1,
        m: &M1,
        handler: &SenderHandle<K2, M2>,
    ) -> Option<()> {
        if let Some(t) = (self.f)(k, m) {
            handler.send(origin, t)?;
        }

        Some(())
    }
}

enum InnerMsg<K, M> {
    Close(),
    Msg(K, M),
}

type SendF<K, M> = Box<dyn Fn(&SenderHandle<K, M>) -> ()>;

pub struct Translator<K1, K2, M1, M2> {
    to: Option<(
        mpsc::UnboundedReceiver<InnerMsg<K1, M1>>,
        HashMap<K1, Box<dyn HelperHandler<K1, M1, K2, M2> + Send + Sync>>,
    )>,
    from: Option<(
        mpsc::UnboundedReceiver<InnerMsg<K2, M2>>,
        HashMap<K2, Box<dyn HelperHandler<K2, M2, K1, M1> + Send + Sync>>,
    )>,

    s1: mpsc::UnboundedSender<InnerMsg<K1, M1>>,
    s2: mpsc::UnboundedSender<InnerMsg<K2, M2>>,
}

use std::marker::Unpin;
impl<K1, K2, M1, M2> Translator<K1, K2, M1, M2>
where
    K1: Hash + Eq + Send + Sync + 'static + Unpin,
    K2: Hash + Eq + Send + Sync + 'static + Unpin,
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

    pub fn add_to<F, T: IntoMessage<K2, M2> + Send + Sync + 'static>(&mut self, key: K1, f: F)
    where
        F: Fn(&K1, &M1) -> Option<T> + Send + Sync + 'static,
    {
        self.to
            .as_mut()
            .map(|(_, map)| map.insert(key, Box::new(Helper::new(f))));
    }

    pub fn add_from<F, T: IntoMessage<K1, M1> + Send + Sync + 'static>(&mut self, key: K2, f: F)
    where
        F: Fn(&K2, &M2) -> Option<T> + Send + Sync + 'static,
    {
        self.from
            .as_mut()
            .map(|(_, map)| map.insert(key, Box::new(Helper::new(f))));
    }

    fn attach(
        origin: ReactorID,
        pool: ThreadPool,
        rec: mpsc::UnboundedReceiver<InnerMsg<K2, M2>>,
        s2: mpsc::UnboundedSender<InnerMsg<K1, M1>>,
        map: HashMap<K2, Box<dyn HelperHandler<K2, M2, K1, M1> + Send + Sync>>,
    ) -> Box<dyn FnOnce(SenderHandle<K1, M1>) -> Sender<K1, M1> + Send> {
        Box::new(move |sender| {
            let (tx, rx) = mpsc::unbounded();

            let rx = receiver_handle(rx);

            // translate msg from rx to something and send it to s2
            pool.spawn_ok(
                rx.map(move |item| {
                    if let Some((_, k, m)) = item {
                        InnerMsg::Msg(k, m)
                    } else {
                        InnerMsg::Close()
                    }
                })
                .for_each(move |item| {
                    s2.unbounded_send(item).expect("Send failed");
                    future::ready(())
                })
                .boxed(),
            );

            // pass msg through from rec to sender
            pool.spawn_ok(
                rec.for_each(move |item| {
                    if match item {
                        InnerMsg::Close() => sender.close(origin),
                        InnerMsg::Msg(k, m) => {
                            if let Some(helper) = map.get(&k) {
                                helper.handle(origin, &k, &m, &sender)
                            } else {
                                Some(())
                            }
                        }
                    }
                    .is_none()
                    {
                        trace!("Couldn't send message");
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
    ) -> Box<dyn FnOnce(SenderHandle<K1, M1>) -> Sender<K1, M1> + Send> {
        let (rec, map) = std::mem::replace(&mut self.from, None).unwrap();

        Translator::<K1, K2, M1, M2>::attach(origin, pool, rec, self.s1.clone(), map)
    }

    pub fn attach_from(
        &mut self,
        origin: ReactorID,
        pool: ThreadPool,
    ) -> Box<dyn FnOnce(SenderHandle<K2, M2>) -> Sender<K2, M2> + Send> {
        let (rec, map) = std::mem::replace(&mut self.to, None).unwrap();

        Translator::<K2, K1, M2, M1>::attach(origin, pool, rec, self.s2.clone(), map)
    }
}
