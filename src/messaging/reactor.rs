use super::types::*;
use capnp::any_pointer;
use capnp::traits::{HasTypeId, Owned};
use std::collections::HashMap;

use tracing::Span;

use errors::ErrorKind::{MozaicError, NoLinkFoundError};
use errors::{self, Consumable, Result};

use core_capnp::{mozaic_message, terminate_stream};

use HasNamedTypeId;

/// Runtime trait
pub trait Ctx: 'static + for<'a> Context<'a> {}
impl<C> Ctx for C where C: 'static + for<'a> Context<'a> {}

pub trait Context<'a>: Sized {
    type Handle: 'a + CtxHandle<Self>;
}

pub trait CtxHandle<C> {
    fn dispatch_internal(&mut self, message: Message) -> Result<()>;
    fn dispatch_external(&mut self, message: Message) -> Result<()>;

    fn open_link<S>(&mut self, params: LinkParams<S, C>) -> Result<()>
    where
        S: 'static + Send,
        C: Ctx;

    fn close_link(&mut self, id: &ReactorId) -> Result<()>;

    fn spawn<S>(&mut self, params: CoreParams<S, C>, name: &str) -> Result<ReactorId>
    where
        S: 'static + Send,
        C: Ctx;
}

/// A reactor is an "actor" in the MOZAIC system. It is defined by an identity
/// (an UUID), a state, and a set of message ('event') handlers.
/// Reactors can communicate by opening links between them. These links
/// are 'tiny reactors', that can set handlers on both internal messages and
/// messages received from the remote party. They can also emit messages towards
/// both parties, acting as a 'proxy' between the reactor states.
/// Note how this is similar to the model/controller type architecture prevalent
/// in web frameworks, where a controller is created for each request (= a link)
/// which then manipulates the model (the core reactor state).
pub struct Reactor<S, C: Ctx> {
    pub id: ReactorId,
    pub internal_state: S,
    pub internal_handlers: HashMap<u64, (CoreHandler<S, C, (), errors::Error>, Span)>,
    pub links: HashMap<ReactorId, (Link<C>, Span)>,
}

impl<S, C: Ctx> Reactor<S, C> {
    // receive a foreign message and send it to the appropriate
    // immigration bureau
    pub fn handle_external_message<'a, 'c: 'a>(
        &'a mut self,
        ctx_handle: &'a mut <C as Context<'c>>::Handle,
        message: Message,
    ) -> Result<()> {
        let reader = message.reader();
        let msg = reader.get()?;
        let sender_uuid = msg.get_sender()?.into();

        let receiver: ReactorId = msg.get_receiver()?.into();

        let (link, span) = self
            .links
            .get_mut(&sender_uuid)
            .ok_or(errors::Error::from_kind(NoLinkFoundError(
                sender_uuid.clone(),
                receiver.clone(),
            )))?;
        let _guard = span.enter();

        let closed = {
            let mut reactor_handle = ReactorHandle {
                id: &self.id,
                ctx: ctx_handle,
            };

            link.handle_external(&mut reactor_handle, msg)?;

            link.link_state.local_closed && link.link_state.remote_closed
        };

        if closed {
            trace!("Closing link");
            ctx_handle.close_link(&sender_uuid)?;
        }

        // the handling link may now emit a domestic message, which will
        // be received by the reactor core and all other links.

        // for example, suppose a game client sends a "command" message,
        // which contains an input json. The link could receive the message,
        // parse the json, and if correct output a "move" event, which can
        // then update the game state residing in the 'core' type.
        // The core handler can then again emit a 'game state changed' event,
        // which the link handlers can pick up on, and forward to their remote
        // parties (the game clients).
        return Ok(());
    }

    pub fn handle_internal_message<'c>(
        &mut self,
        ctx_handle: &mut <C as Context<'c>>::Handle,
        message: Message,
    ) -> Result<()> {
        let reader = message.reader();

        let msg = reader.get()?;

        if let Some((handler, span)) = self.internal_handlers.get(&msg.get_type_id()) {
            let _guard = span.enter();
            trace!("Handling type {}", msg.get_type_id());

            let mut reactor_handle = ReactorHandle {
                id: &self.id,
                ctx: ctx_handle,
            };

            let mut handler_ctx = HandlerCtx {
                state: &mut self.internal_state,
                handle: &mut reactor_handle,
            };

            handler.handle(&mut handler_ctx, msg.get_payload())?;
        }

        for (link, span) in self.links.values_mut() {
            let _guard = span.enter();

            let mut reactor_handle = ReactorHandle {
                id: &self.id,
                ctx: ctx_handle,
            };

            link.handle_internal(&mut reactor_handle, msg).display();
        }

        return Ok(());
    }

    pub fn destroy(&mut self) -> Result<()> {
        for (_id, _link) in self.links.iter() {}

        Ok(())
    }

    // helper function for implementing runtimes, should go sometime soon
    pub fn handle<'a, 'c>(
        &'a self,
        ctx: &'a mut <C as Context<'c>>::Handle,
    ) -> ReactorHandle<'a, 'c, C> {
        ReactorHandle { id: &self.id, ctx }
    }
}

pub struct LinkState {
    pub local_closed: bool,
    pub remote_closed: bool,
}

pub struct Link<C> {
    pub remote_id: ReactorId,

    pub reducer: Box<dyn LinkReducerTrait<C>>,

    pub link_state: LinkState,
}

impl<C> Link<C>
where
    C: Ctx,
{
    fn handle_external(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: mozaic_message::Reader,
    ) -> Result<()> {
        trace!("Link external handling {}", msg.get_type_id());

        if msg.get_type_id() == terminate_stream::Reader::type_id() {
            self.link_state.remote_closed = true;
            self.link_state.local_closed = true; // What is this?
            handle.ctx.close_link(&self.remote_id)?;
            return Ok(());
        }

        let mut link_handle = handle.link_handle(&self.remote_id, &mut self.link_state);

        return self.reducer.handle_external(&mut link_handle, msg);
    }

    fn handle_internal(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: mozaic_message::Reader,
    ) -> Result<()> {
        trace!("Link internal handling {}", msg.get_type_id());

        let mut link_handle = handle.link_handle(&self.remote_id, &mut self.link_state);
        return self.reducer.handle_internal(&mut link_handle, msg);
    }
}

pub struct LinkReducer<S, C>
where
    C: Ctx,
{
    /// handler state
    pub state: S,
    /// handle internal messages (sent by core, or other link handlers)
    pub internal_handlers: LinkHandlers<S, C, (), errors::Error>,
    /// handle external messages (sent by remote party)
    pub external_handlers: LinkHandlers<S, C, (), errors::Error>,
}

/// A trait for all LinkReducers to implement.
/// We need this to make boxed LinkReducers (with the state type erased).
pub trait LinkReducerTrait<C: Ctx>: 'static + Send {
    fn handle_external<'a, 'b>(
        &'a mut self,
        link_handle: &'a mut LinkHandle<'a, '_, C>,
        msg: mozaic_message::Reader<'a>,
    ) -> Result<()>;

    fn handle_internal<'a>(
        &'a mut self,
        link_handle: &'a mut LinkHandle<'a, '_, C>,
        msg: mozaic_message::Reader<'a>,
    ) -> Result<()>;
}

impl<S, C> LinkReducerTrait<C> for LinkReducer<S, C>
where
    S: 'static + Send,
    C: 'static + Ctx,
{
    fn handle_external<'a>(
        &'a mut self,
        link_handle: &'a mut LinkHandle<'a, '_, C>,
        msg: mozaic_message::Reader<'a>,
    ) -> Result<()> {
        let (handler, span) =
            self.external_handlers
                .get(&msg.get_type_id())
                .ok_or(errors::Error::from_kind(MozaicError(
                    "No link external handler found",
                )))?;

        let _guard = span.enter();
        trace!("handle external {}", msg.get_type_id());

        let mut ctx = HandlerCtx {
            state: &mut self.state,
            handle: link_handle,
        };

        handler.handle(&mut ctx, msg.get_payload())?;

        Ok(())
    }

    fn handle_internal<'a>(
        &'a mut self,
        link_handle: &'a mut LinkHandle<'a, '_, C>,
        msg: mozaic_message::Reader<'a>,
    ) -> Result<()> {
        if let Some((handler, span)) = self.internal_handlers.get(&msg.get_type_id()) {
            let _guard = span.enter();
            trace!("handle internal");

            let mut ctx = HandlerCtx {
                state: &mut self.state,
                handle: link_handle,
            };
            handler.handle(&mut ctx, msg.get_payload())?;
        }

        Ok(())
    }
}

/// Handle for manipulating a reactor.
pub struct ReactorHandle<'a, 'c: 'a, C: Ctx> {
    id: &'a ReactorId,
    ctx: &'a mut <C as Context<'c>>::Handle,
}

impl<'a, 'c, C: Ctx> ReactorHandle<'a, 'c, C> {
    fn link_handle<'b>(
        &'b mut self,
        remote_id: &'b ReactorId,
        link_state: &'b mut LinkState,
    ) -> LinkHandle<'b, 'c, C> {
        LinkHandle {
            id: self.id,
            ctx: self.ctx,
            link_state,
            remote_id,
        }
    }

    pub fn id(&self) -> &'a ReactorId {
        self.id
    }

    pub fn send_internal<T>(&mut self, msg_buffer: MsgBuffer<T>) -> Result<()> {
        let mut builder = msg_buffer.into_builder();
        {
            let mut msg: mozaic_message::Builder = builder.get_root()?;

            msg.set_sender(self.id.bytes());
            msg.set_receiver(self.id.bytes());
        }

        let message = Message::from_capnp(builder.into_reader());
        self.ctx.dispatch_internal(message)
    }

    pub fn spawn<S>(&mut self, params: CoreParams<S, C>, name: &str) -> Result<ReactorId>
    where
        S: 'static + Send,
    {
        self.ctx.spawn(params, name)
    }

    pub fn open_link<S>(&mut self, params: LinkParams<S, C>) -> Result<()>
    where
        S: 'static + Send,
    {
        self.ctx.open_link(params)
    }
}

/// Handle for manipulating a link.
pub struct LinkHandle<'a, 'c: 'a, C: Ctx> {
    id: &'a ReactorId,
    remote_id: &'a ReactorId,
    link_state: &'a mut LinkState,
    ctx: &'a mut <C as Context<'c>>::Handle,
}

impl<'a, 'c, C: Ctx> LinkHandle<'a, 'c, C> {
    pub fn id(&self) -> &'a ReactorId {
        self.id
    }

    pub fn remote_uuid(&self) -> &'a ReactorId {
        self.remote_id
    }

    pub fn send_internal<T>(&mut self, msg_buffer: MsgBuffer<T>) -> Result<()> {
        let mut builder = msg_buffer.into_builder();
        {
            let mut msg: mozaic_message::Builder = builder.get_root()?;

            msg.set_sender(self.id.bytes());
            msg.set_receiver(self.id.bytes());
        }

        let message = Message::from_capnp(builder.into_reader());
        self.ctx.dispatch_internal(message)
    }

    /// ? Send message to the other side of the link
    pub fn send_message<T>(&mut self, msg_buffer: MsgBuffer<T>) -> Result<()> {
        let mut builder = msg_buffer.into_builder();
        {
            let mut msg: mozaic_message::Builder = builder.get_root()?;

            msg.set_sender(self.id.bytes());
            msg.set_receiver(self.remote_id.bytes());
        }

        let message = Message::from_capnp(builder.into_reader());
        self.ctx.dispatch_external(message)
    }

    pub fn close_link(&mut self) -> Result<()> {
        if self.link_state.local_closed {
            return Ok(());
        }

        if self.link_state.remote_closed {
            return self._close_link();
        }

        let msg = MsgBuffer::<terminate_stream::Owned>::new();
        self.send_message(msg).display();
        self._close_link()
    }

    pub fn close_link_hard(&mut self) -> Result<()> {
        self.link_state.remote_closed = true;
        self._close_link()
    }

    fn _close_link(&mut self) -> Result<()> {
        self.link_state.local_closed = true;

        self.ctx.close_link(&self.remote_id)?;
        Ok(())
    }
}

/// A simple struct for bundling a borrowed state and handle.
pub struct HandlerCtx<'a, S, H> {
    state: &'a mut S,
    handle: &'a mut H,
}

impl<'a, S, H> HandlerCtx<'a, S, H> {
    pub fn state<'b>(&'b mut self) -> &'b mut S {
        &mut self.state
    }

    pub fn handle<'b>(&'b mut self) -> &'b mut H {
        &mut self.handle
    }

    pub fn split<'b>(&'b mut self) -> (&'b mut S, &'b mut H) {
        (&mut self.state, &mut self.handle)
    }
}

use std::marker::PhantomData;

pub struct CtxHandler<M, F> {
    message_type: PhantomData<M>,
    function: F,
}

impl<M, F> CtxHandler<M, F> {
    pub fn new(function: F) -> Self {
        CtxHandler {
            message_type: PhantomData,
            function,
        }
    }
}

impl<'a, S, H, M, F, T, E> Handler<'a, HandlerCtx<'a, S, H>, M> for CtxHandler<M, F>
where
    F: Fn(&mut S, &mut H, <M as Owned<'a>>::Reader) -> std::result::Result<T, E>,
    F: Send,
    M: Owned<'a> + 'static + Send,
{
    type Output = T;
    type Error = E;

    fn handle(
        &self,
        ctx: &mut HandlerCtx<'a, S, H>,
        reader: <M as Owned<'a>>::Reader,
    ) -> std::result::Result<T, E> {
        let (state, handle) = ctx.split();
        (self.function)(state, handle, reader)
    }
}

pub type ReactorCtx<'a, 'c, S, C> = HandlerCtx<'a, S, ReactorHandle<'a, 'c, C>>;
pub type LinkCtx<'a, 'c, S, C> = HandlerCtx<'a, S, LinkHandle<'a, 'c, C>>;

use std::ops::Deref;

impl<'a, S, H> Deref for HandlerCtx<'a, S, H> {
    type Target = S;

    fn deref(&self) -> &S {
        &self.state
    }
}

type CoreHandler<S, C, T, E> = Box<
    dyn for<'a, 'c> Handler<
        'a,
        HandlerCtx<'a, S, ReactorHandle<'a, 'c, C>>,
        any_pointer::Owned,
        Output = T,
        Error = E,
    >,
>;

type LinkHandler<S, C, T, E> = Box<
    dyn for<'a, 'c> Handler<
        'a,
        HandlerCtx<'a, S, LinkHandle<'a, 'c, C>>,
        any_pointer::Owned,
        Output = T,
        Error = E,
    >,
>;

type LinkHandlers<S, C, T, E> = HashMap<u64, (LinkHandler<S, C, T, E>, Span)>;

// *********** SPAWNING **********

/// ? CoreParams are the parameters for defining handlers for the reactor core
pub struct CoreParams<S, C: Ctx> {
    pub state: S,
    pub handlers: HashMap<u64, (CoreHandler<S, C, (), errors::Error>, Span)>,
}

impl<S, C: Ctx> CoreParams<S, C> {
    pub fn new(state: S) -> Self {
        CoreParams {
            state,
            handlers: HashMap::new(),
        }
    }

    pub fn handler<M, H>(&mut self, _m: M, h: H)
    where
        M: for<'a> Owned<'a> + Send + 'static,
        <M as Owned<'static>>::Reader: HasNamedTypeId,
        H: 'static
            + for<'a, 'c> Handler<
                'a,
                HandlerCtx<'a, S, ReactorHandle<'a, 'c, C>>,
                M,
                Output = (),
                Error = errors::Error,
            >,
    {
        let boxed = Box::new(AnyPtrHandler::new(h));
        let span = span!(tracing::Level::INFO, "handler", id = M::Reader::get_name());

        self.handlers
            .insert(<M as Owned<'static>>::Reader::type_id(), (boxed, span));
    }
}

/// ? LinkParams are the parameters for defining handlers for links.
/// ? Internal are sent to the reactor, 'to it's params'
/// ? External handlers are sent to links of other reactors
pub struct LinkParams<S, C: Ctx> {
    pub remote_id: ReactorId,
    pub state: S,
    pub internal_handlers: LinkHandlers<S, C, (), errors::Error>,
    pub external_handlers: LinkHandlers<S, C, (), errors::Error>,
}

impl<S, C: Ctx> LinkParams<S, C> {
    pub fn new(remote_id: ReactorId, state: S) -> Self {
        LinkParams {
            remote_id,
            state,
            internal_handlers: HashMap::new(),
            external_handlers: HashMap::new(),
        }
    }

    pub fn internal_handler<M, H>(&mut self, _m: M, h: H)
    where
        M: for<'a> Owned<'a> + Send + 'static,
        <M as Owned<'static>>::Reader: HasNamedTypeId,
        H: 'static
            + for<'a, 'c> Handler<
                'a,
                HandlerCtx<'a, S, LinkHandle<'a, 'c, C>>,
                M,
                Output = (),
                Error = errors::Error,
            >,
    {
        let boxed = Box::new(AnyPtrHandler::new(h));
        let span = span!(
            tracing::Level::INFO,
            "I_handler",
            id = M::Reader::get_name()
        );

        self.internal_handlers
            .insert(<M as Owned<'static>>::Reader::type_id(), (boxed, span));
    }

    pub fn external_handler<M, H>(&mut self, _m: M, h: H)
    where
        M: for<'a> Owned<'a> + Send + 'static,
        <M as Owned<'static>>::Reader: HasNamedTypeId,
        H: 'static
            + for<'a, 'c> Handler<
                'a,
                HandlerCtx<'a, S, LinkHandle<'a, 'c, C>>,
                M,
                Output = (),
                Error = errors::Error,
            >,
    {
        let boxed = Box::new(AnyPtrHandler::new(h));
        let span = span!(
            tracing::Level::INFO,
            "E_handler",
            id = M::Reader::get_name(),
            to = tracing::field::debug(self.remote_id.clone())
        );

        self.external_handlers
            .insert(<M as Owned<'static>>::Reader::type_id(), (boxed, span));
    }
}

pub trait LinkParamsTrait<C: Ctx>: 'static + Send {
    fn remote_id<'a>(&'a self) -> &'a ReactorId;
    fn into_link(self: Box<Self>) -> Link<C>;
}

impl<S, C> LinkParamsTrait<C> for LinkParams<S, C>
where
    S: 'static + Send,
    C: Ctx + 'static,
{
    fn remote_id<'a>(&'a self) -> &'a ReactorId {
        &self.remote_id
    }

    fn into_link(self: Box<Self>) -> Link<C> {
        let unboxed = *self;

        let link_state = LinkState {
            local_closed: false,
            remote_closed: false,
        };

        let reducer = LinkReducer {
            state: unboxed.state,
            internal_handlers: unboxed.internal_handlers,
            external_handlers: unboxed.external_handlers,
        };

        return Link {
            remote_id: unboxed.remote_id,
            reducer: Box::new(reducer),
            link_state,
        };
    }
}
