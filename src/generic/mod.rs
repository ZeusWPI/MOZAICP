use futures::channel::mpsc;

use std::any;
use std::hash::Hash;
use std::marker::PhantomData;

mod message;
pub use self::message::{JSONMessage, Message, Typed};
mod broker;
mod link;
mod reactor;
mod types;
pub use broker::BrokerHandle;

pub use self::link::{Link, LinkHandle, LinkParams};
pub use self::reactor::{CoreParams, Reactor, ReactorHandle, ReactorState, TargetReactor};

// ! Just some types to make things organised
pub use self::types::ReactorID;

// pub struct FunctionHandler<F, S, R, T, M>

pub fn i_to_e<S, T: Clone + 'static + IntoMessage<any::TypeId, Message>>(
) -> impl 'static + Fn(&mut S, &mut LinkHandle<any::TypeId, Message>, &T) -> () {
    |_, handle, t| {
        handle.send_message(t.clone());
    }
}

pub fn e_to_i<S, T: Clone + 'static + IntoMessage<any::TypeId, Message>>(
    target: TargetReactor,
) -> impl 'static + Fn(&mut S, &mut LinkHandle<any::TypeId, Message>, &T) -> () {
    move |_, handle, t| {
        handle.send_internal(t.clone(), target);
    }
}

pub struct Initialize();

/// Shortcut types
pub type Sender<K, M> = mpsc::UnboundedSender<Operation<K, M>>;
pub type Receiver<K, M> = mpsc::UnboundedReceiver<Operation<K, M>>;

///
/// Main handler trait
/// This should apply one message to S
/// And use handle specific functions
///
pub trait Handler<S, H, M> {
    fn handle(&mut self, s: &mut S, h: &mut H, m: M);
}

pub type LinkSpawner<K, M> = Box<
    dyn FnOnce(
            Handles<K, M>,
        ) -> Box<
            dyn for<'a, 'b> Handler<(), ReactorHandle<'b, K, M>, &'a mut LinkOperation<'a, K, M>>
                + Send,
        > + Send,
>;

/// (SourceHandle, TargetHandle, SourceId, TargetID)
/// This is used to not expose Operation, not really working though lol
type Handles<K, M> = (Sender<K, M>, Sender<K, M>, ReactorID, ReactorID);

pub enum LinkOperation<'a, K, M> {
    InternalMessage(&'a K, &'a mut M),
    ExternalMessage(&'a K, &'a mut M),
    Close(),
}

///
/// The actual messages that are sent
/// These get consumed by the reactors
///
pub enum Operation<K, M> {
    InternalMessage(K, M, TargetReactor),
    ExternalMessage(ReactorID, K, M),
    Close(),
    OpenLink(ReactorID, LinkSpawner<K, M>, bool),
    CloseLink(ReactorID),
}

pub trait FromMessage<K, M>
where
    Self: Sized,
{
    fn from_msg<'a>(key: &K, msg: &'a mut M) -> Option<&'a Self>;
}

pub trait IntoMessage<K, M> {
    fn into_msg(self) -> Option<(K, M)>;
}

pub trait Key<K> {
    fn key() -> K;
}

impl<T: 'static> Key<any::TypeId> for T {
    fn key() -> any::TypeId {
        any::TypeId::of::<T>()
    }
}

pub struct SenderHandle<K, M> {
    sender: Sender<K, M>,
}

impl<K, M> SenderHandle<K, M>
where
    K: 'static + Eq + Hash + Send + Unpin,
    M: 'static + Send,
{
    pub fn send<T: IntoMessage<K, M>>(&self, from: ReactorID, msg: T) -> Option<()> {
        let (k, m) = msg.into_msg()?;
        self.sender
            .unbounded_send(Operation::ExternalMessage(from, k, m))
            .ok()?;

        Some(())
    }

    pub fn close(&self, from: ReactorID) -> Option<()> {
        self.sender
            .unbounded_send(Operation::CloseLink(from))
            .ok()?;

        Some(())
    }
}

impl<K, M> Clone for SenderHandle<K, M> {
    fn clone(&self) -> Self {
        SenderHandle {
            sender: self.sender.clone(),
        }
    }
}

use futures::stream::{Stream, StreamExt};
pub fn receiver_handle<K, M>(
    inner: Receiver<K, M>,
) -> impl Stream<Item = Option<(ReactorID, K, M)>> {
    inner.filter_map(move |item| async {
        match item {
            Operation::ExternalMessage(id, k, m) => Some(Some((id, k, m))),
            _ => Some(None),
            // _ => None,
        }
    })
}

///
/// FunctionHandler<S, T, F, R> makes a Handler from a function
/// For Messages that is
///
pub struct FunctionHandler<F, S, R, T, M>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send,
{
    phantom: PhantomData<(S, R, T, M)>,
    function: F,
}

impl<F, S, R, T, M> FunctionHandler<F, S, R, T, M>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send,
    M: 'static + Send,
{
    pub fn from(function: F) -> Self {
        Self {
            phantom: PhantomData,
            function,
        }
    }
}

impl<F, S, R, T, M, K> Into<(K, Self)> for FunctionHandler<F, S, R, T, M>
where
    F: 'static + Send + Fn(&mut S, &mut R, &T) -> (),
    S: 'static + Send,
    R: 'static + Send,
    T: 'static + Send + Key<K>,
    M: 'static + Send,
    K: 'static + Send,
{
    fn into(self) -> (K, Self) {
        (T::key(), self)
    }
}

///
/// This is just stupid, you shouldn't have to implement Handler for ReactorHandle and LinkHandle
/// but this for<'b> is fucking the compiler up.
///
/// As long as no fix is found, this will have to do
/// It is just stupid for every M type, here Message you would have to implement it twice
/// though it is the same implementation
///
/// For clarification, this implementation goes from a generic Message
/// to a specific T that is expected for F
///
impl<'a, K, F, S, T, M> Handler<S, ReactorHandle<'a, K, M>, (&K, &mut M)>
    for FunctionHandler<F, S, ReactorHandle<'_, K, M>, T, M>
where
    F: 'static + Send + for<'b> Fn(&mut S, &mut ReactorHandle<'b, K, M>, &T) -> (),
    S: 'static + Send,
    T: 'static + Send + FromMessage<K, M>,
    K: 'static + Send,
    M: 'static + Send,
{
    fn handle<'b>(
        &mut self,
        state: &mut S,
        handle: &mut ReactorHandle<'b, K, M>,
        msg: (&K, &mut M),
    ) {
        let (key, message) = msg;
        T::from_msg(key, message)
            .map(|item| (self.function)(state, handle, &item))
            .expect("No message found at pointer location");
    }
}

impl<'a, K, F, S, T, M> Handler<S, LinkHandle<'a, K, M>, (&K, &mut M)>
    for FunctionHandler<F, S, LinkHandle<'_, K, M>, T, M>
where
    F: 'static + Send + for<'b> Fn(&mut S, &mut LinkHandle<'b, K, M>, &T) -> (),
    S: 'static + Send,
    T: 'static + Send + FromMessage<K, M>,
    K: 'static + Send,
    M: 'static + Send,
{
    fn handle<'b>(&mut self, state: &mut S, handle: &mut LinkHandle<'b, K, M>, msg: (&K, &mut M)) {
        let (key, message) = msg;
        T::from_msg(key, message)
            .map(|item| (self.function)(state, handle, &item))
            .expect("No message found at pointer location");
    }
}
