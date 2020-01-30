use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::pin::Pin;

use tokio::net::{TcpStream};
use tokio::sync::mpsc;

use futures::task::{Context as Ctx, Poll};
use futures::{Future};
use futures::executor;

use capnp;
use capnp::any_pointer;
use capnp::message::{Builder, HeapAllocator, Reader, ReaderOptions, ReaderSegments};
use capnp::traits::{HasTypeId, Owned};

use crate::errors;
use crate::messaging::types::{AnyPtrHandler, Handler, Message};
use crate::network_capnp::{disconnected, network_message, publish};
use crate::runtime::BrokerHandle;

pub struct ConnectionHandler<S> {
    handler: StreamHandler<S>,
    rx: mpsc::UnboundedReceiver<Message>,
}

impl<S> ConnectionHandler<S> {
    pub fn new<F>(stream: TcpStream, broker: BrokerHandle, core_constructor: F) -> Self
    where
        F: FnOnce(mpsc::UnboundedSender<Message>) -> HandlerCore<S>,
    {
        let (tx, rx) = mpsc::unbounded_channel();

        let core = core_constructor(tx);

        return ConnectionHandler {
            handler: StreamHandler::new(core, broker, stream),
            rx,
        };
    }

    /// Wrap generic message in network_capnp::publish and send it over the wire
    fn forward_messages(&mut self) -> Poll<()> {
        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    self.handler.writer().write(publish::Owned, |b| {
                        let mut publish: publish::Builder = b.init_as();
                        publish.set_message(msg.bytes());
                    });
                }
                Err(mpsc::error::TryRecvError::Empty) => return Poll::Pending,
                Err(mpsc::error::TryRecvError::Closed) => return Poll::Ready(()),
            }
        }
    }

    pub fn writer<'a>(&'a mut self) -> Writer<'a> {
        self.handler.writer()
    }
}

impl<S> Future for ConnectionHandler<S>
    where S: Unpin {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Ctx) -> Poll<()> {
        let this = Pin::into_inner(self);

        if this.forward_messages().is_ready() {
            debug!("Rx closed at connection handler");
            return Poll::Ready(());
        }

        if this
            .handler
            .poll_stream(ctx)
            // .unwrap_or(Poll::Ready(()))
            .is_ready()
        {
            let mut msg = Builder::new_default();

            {
                let mut root = msg.init_root::<network_message::Builder>();
                root.set_type_id(disconnected::Builder::type_id());
            }

            this.handler
                .handle_message(msg.into_reader())
                .expect("what the hell");

            debug!("Tcp streamed closed at connection handler");
            return Poll::Ready(());
        }

        return Poll::Pending;
    }
}

pub struct HandlerCore<S> {
    state: S,
    handlers: HashMap<u64, MessageHandler<S>>,
}

impl<S> HandlerCore<S> {
    pub fn new(state: S) -> Self {
        HandlerCore {
            state,
            handlers: HashMap::new(),
        }
    }

    pub fn on<M, H>(&mut self, _m: M, h: H)
    where
        M: for<'a> Owned<'a> + Send + 'static,
        <M as Owned<'static>>::Reader: HasTypeId,
        H: 'static
            + for<'a, 'c> Handler<'a, MsgHandlerCtx<'a, 'c, S>, M, Output = (), Error = errors::Error>,
    {
        let boxed = Box::new(AnyPtrHandler::new(h));
        self.handlers
            .insert(<M as Owned<'static>>::Reader::type_id(), boxed);
    }

    pub fn handle(
        &mut self,
        writer: &mut Writer,
        r: network_message::Reader,
    ) -> Result<(), errors::Error> {
        let type_id = r.get_type_id();
        match self.handlers.get(&type_id) {
            None => error!("unknown message type"),
            Some(handler) => {
                let mut ctx = MsgHandlerCtx {
                    state: &mut self.state,
                    writer,
                };
                handler.handle(&mut ctx, r.get_data())?;
            }
        }
        return Ok(());
    }
}

pub struct StreamHandler<S> {
    stream: TcpStream,
    write_queue: VecDeque<Builder<HeapAllocator>>,
    core: HandlerCore<S>,
    broker: BrokerHandle,
}

use capnp_futures::serialize;

impl<S> StreamHandler<S> {
    pub fn new(core: HandlerCore<S>, broker: BrokerHandle, stream: TcpStream) -> Self {
        StreamHandler {
            stream,
            write_queue: VecDeque::new(),
            core,
            broker,
        }
    }

    pub fn writer<'a>(&'a mut self) -> Writer<'a> {
        Writer {
            write_queue: &mut self.write_queue,
        }
    }

    fn flush_writes(&mut self, ctx: &mut Ctx) -> Poll<()> {

        while let Some(builder) = self.write_queue.pop_front() {
            let (_, w) = self.stream.split();
            if executor::block_on(serialize::write_message(&mut AWrite(w), builder)).is_err() {
                eprintln!("Capnp failed");
                return Poll::Ready(());
            }
        }

        let (_, w) = self.stream.split();
        return AsyncWrite::poll_flush(Pin::new(&mut AWrite(w)), ctx).map(|_| ());
    }

    pub fn handle_message<T>(&mut self, builder: Reader<T>) -> Result<(), errors::Error>
    where
        T: ReaderSegments,
    {
        let mut writer = Writer {
            write_queue: &mut self.write_queue,
        };
        let reader = builder.get_root::<network_message::Reader>()?;

        return self.core.handle(&mut writer, reader);
    }

    // Stream.poll() -> Result<Async<T>, errors::Error>
    /// self.transport.poll will finish with Err(disconnected), I think
    fn poll_transport(&mut self, ctx: &mut Ctx) -> Poll<()> {
        loop {
            let (r, _) = self.stream.split();
            if let Ok(item) = executor::block_on(serialize::read_message(ARead(r), ReaderOptions::new())) {
                if let Some(reader) = item {
                    if self.handle_message(reader).is_err() {
                        eprintln!("Capnp failed");
                        return Poll::Ready(());
                    };

                    ready!(self.flush_writes(ctx));
                } else {
                    return Poll::Pending;
                }
            } else {
                return Poll::Ready(());
            }
        }
    }

    fn poll_stream(&mut self, ctx: &mut Ctx) -> Poll<()> {
        ready!(self.flush_writes(ctx));
        return self.poll_transport(ctx);
    }
}

type MessageHandler<S> = Box<
    dyn for<'a, 'c> Handler<
        'a,
        MsgHandlerCtx<'a, 'c, S>,
        any_pointer::Owned,
        Output = (),
        Error = errors::Error,
    >,
>;

pub struct MsgHandlerCtx<'a, 'w, S> {
    pub state: &'a mut S,
    pub writer: &'a mut Writer<'w>,
}

pub struct MsgHandler<M, F> {
    message_type: PhantomData<M>,
    function: F,
}

impl<M, F> MsgHandler<M, F> {
    pub fn new(function: F) -> Self {
        MsgHandler {
            message_type: PhantomData,
            function,
        }
    }
}

impl<'a, 'c, S, M, F, T, E> Handler<'a, MsgHandlerCtx<'a, 'c, S>, M> for MsgHandler<M, F>
where
    F: Fn(&mut S, &mut Writer, <M as Owned<'a>>::Reader) -> Result<T, E>,
    F: Send,
    M: Owned<'a> + 'static + Send,
{
    type Output = T;
    type Error = E;

    fn handle(
        &self,
        ctx: &mut MsgHandlerCtx<'a, 'c, S>,
        reader: <M as Owned<'a>>::Reader,
    ) -> Result<T, E> {
        let MsgHandlerCtx { state, writer } = ctx;
        (self.function)(state, writer, reader)
    }
}

// TODO: replace this with a more general handle
pub struct Writer<'a> {
    write_queue: &'a mut VecDeque<Builder<HeapAllocator>>,
}

impl<'a> Writer<'a> {
    pub fn write<M, F>(&mut self, _m: M, initializer: F)
    where
        F: for<'b> FnOnce(capnp::any_pointer::Builder<'b>),
        M: Owned<'static>,
        <M as Owned<'static>>::Builder: HasTypeId,
    {
        let mut builder = Builder::new_default();
        {
            let mut msg = builder.init_root::<network_message::Builder>();
            msg.set_type_id(<M as Owned<'static>>::Builder::type_id());
            {
                let payload_builder = msg.reborrow().init_data();
                initializer(payload_builder);
            }
        }
        self.write_queue.push_back(builder);
    }
}

use futures::{AsyncRead, AsyncWrite};
use std::marker::Unpin;
struct AWrite<A: tokio::io::AsyncWrite + Unpin>(A);

impl<A: tokio::io::AsyncWrite + Unpin> AsyncWrite for AWrite<A> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut futures::task::Context,
        buf: &[u8],
    ) -> Poll<Result<usize, futures::io::Error>> {
        tokio::io::AsyncWrite::poll_write(Pin::new(&mut Pin::into_inner(self).0), cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut futures::task::Context,
    ) -> Poll<Result<(), futures::io::Error>> {
        tokio::io::AsyncWrite::poll_flush(Pin::new(&mut Pin::into_inner(self).0), cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Ctx) -> Poll<Result<(), futures::io::Error>> {
        tokio::io::AsyncWrite::poll_shutdown(Pin::new(&mut Pin::into_inner(self).0), cx)
    }
}

struct ARead<A: tokio::io::AsyncRead + Unpin>(A);
impl<A: tokio::io::AsyncRead + Unpin> AsyncRead for ARead<A> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut futures::task::Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, futures::io::Error>> {
        tokio::io::AsyncRead::poll_read(Pin::new(&mut Pin::into_inner(self).0), cx, buf)
    }
}
