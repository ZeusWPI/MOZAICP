extern crate mozaic;
extern crate hex;
extern crate tokio;
#[macro_use]
extern crate futures;
extern crate rand;
extern crate cursive;
extern crate capnp;

use tokio::net::TcpStream;
use futures::sync::mpsc;
use futures::Future;

use mozaic::core_capnp::{initialize, terminate_stream};
use mozaic::chat_capnp;
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::client::{LinkHandler, RuntimeState};
use mozaic::errors::*;


use std::thread;
use std::env;
use std::sync::{Arc, Mutex};

use cursive::align::VAlign;
use cursive::Cursive;
use cursive::theme::Theme;
use cursive::traits::{Boxable, Identifiable};
use cursive::views::{TextView, EditView, LinearLayout};

// pub mod chat {
//     include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
// }

fn main() {
    let args: Vec<String> = env::args().collect();
    let user = args.get(1).unwrap_or(&"Ben".to_string()).clone();

    // Creates the cursive root - required for every application.
    let mut siv = Cursive::default();

    siv.set_theme({
        let mut t = Theme::default();
        t.shadow = false;
        t
    });

    // ugly chat view
    siv.add_layer(LinearLayout::vertical()
        .child(TextView::empty()
            .v_align(VAlign::Bottom)
            .with_id("messages")
            .full_height())
        .child(EditView::new()
            .with_id("input")
            .full_width())
    );

    let cb_sink = siv.cb_sink().clone();
    thread::spawn(move || {
        let addr = "127.0.0.1:9142".parse().unwrap();
        tokio::run(futures::lazy(move || {
            // This part is needlessly complex, please ignore =/
            let rt: Arc<Mutex<RuntimeState>> = RuntimeState::bootstrap(|runtime| {
                let (tx, rx) = mpsc::unbounded();

                runtime::RuntimeWorker::spawn(rx, cb_sink, runtime);

                return tx;
            });

            TcpStream::connect(&addr)
                .map_err(|err| panic!(err))
                .and_then(move |stream| {
                    LinkHandler::new(stream, rt, move |params| {
                        let r = ClientReactor {
                            greeter_id: params.greeter_id,
                            runtime_id: params.runtime_id,
                            user: user.clone(),
                        };
                        return r.params();
                    })
                })
            }));
    });

    siv.set_fps(10);
    // Starts the event loop.
    siv.run();
}

// Main client logic
/// ? greeter_id is the server, the tcp stream that you connected to
/// ? runtime_id is your own runtime, to handle visualisation etc
/// ? user are you  ...
struct ClientReactor {
    greeter_id: ReactorId,
    runtime_id: ReactorId,
    user: String,
}

impl ClientReactor {
    fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::initialize));
        return params;
    }

    // reactor setup
    fn initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        // open link with chat server
        let link = (ServerLink {}).params(self.greeter_id.clone());
        handle.open_link(link)?;

        // open link with runtime, for communicating with chat GUI
        let runtime_link = (RuntimeLink {user: self.user.clone()}).params(self.runtime_id.clone());
        handle.open_link(runtime_link)?;

        // dispatch this additional message to instruct the runtime link
        // to connect to the gui.
        // TODO: this is kind of initalization code, could it be avoided?
        let msg = MsgBuffer::<chat_capnp::connect_to_gui::Owned>::new();
        handle.send_internal(msg)?;

        return Ok(());
    }
}

// Handler for the connection with the chat server
struct ServerLink {}

impl ServerLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);

        params.external_handler(
            terminate_stream::Owned,
            CtxHandler::new(Self::close_handler),
        );

        params.external_handler(
            chat_capnp::chat_message::Owned,
            CtxHandler::new(Self::receive_chat_message),
        );

        params.internal_handler(
            chat_capnp::send_message::Owned,
            CtxHandler::new(Self::send_chat_message),
        );

        return params;
    }

    // pick up a 'send_message' event from the reactor, and put it to effect
    // by constructing the chat message and sending it to the chat server.
    fn send_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        send_message: chat_capnp::send_message::Reader,
    ) -> Result<()>
    {
        let message = send_message.get_message()?;
        let user = send_message.get_user()?;

        let mut chat_message = MsgBuffer::<chat_capnp::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(message);
            b.set_user(user);
        });

        handle.send_message(chat_message)?;

        return Ok(());

    }

    // receive a chat message from the chat server, and broadcast it on the
    // reactor.
    fn receive_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        chat_message: chat_capnp::chat_message::Reader,
    ) -> Result<()>
    {
        let message = chat_message.get_message()?;
        let user = chat_message.get_user()?;

        let mut chat_message = MsgBuffer::<chat_capnp::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(message);
            b.set_user(user);
        });
        handle.send_internal(chat_message)?;

        return Ok(());
    }

    fn close_handler<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: terminate_stream::Reader,
    ) -> Result<()>
    {
        // also close our end of the stream
        handle.close_link()?;
        return Ok(());
    }

}

//
struct RuntimeLink {
    user: String,
}

impl RuntimeLink {
    fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.internal_handler(
            chat_capnp::connect_to_gui::Owned,
            CtxHandler::new(Self::connect_to_gui),
        );

        params.internal_handler(
            chat_capnp::chat_message::Owned,
            CtxHandler::new(Self::handle_chat_message),
        );

        params.external_handler(
            chat_capnp::user_input::Owned,
            CtxHandler::new(Self::handle_user_input),
        );

        return params;
    }

    // Send a message to the GUI controller to subscribe to input events.
    fn connect_to_gui<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: chat_capnp::connect_to_gui::Reader,
    ) -> Result<()>
    {
        let connect = MsgBuffer::<chat_capnp::connect_to_gui::Owned>::new();
        handle.send_message(connect)?;
        return Ok(());
    }

    // Pick up chat messages on the reactor, and forward them to the chat GUI.
    fn handle_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        chat_message: chat_capnp::chat_message::Reader,
    ) -> Result<()>
    {
        let message = chat_message.get_message()?;
        let user = chat_message.get_user()?;

        let mut chat_message = MsgBuffer::<chat_capnp::chat_message::Owned>::new();
        chat_message.build(|b| {
            b.set_message(message);
            b.set_user(user);
        });
        handle.send_message(chat_message)?;

        return Ok(());
    }

    fn handle_user_input<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        input: chat_capnp::user_input::Reader,
    ) -> Result<()>
    {
        let message = input.get_text()?;

        let mut send_message = MsgBuffer::<chat_capnp::send_message::Owned>::new();
        send_message.build(|b| {
            b.set_message(message);
            b.set_user(&self.user);
        });
        handle.send_internal(send_message)?;

        return Ok(());
    }
}

// this code is hacked together, don't pay too much attention to it.
mod runtime {
    use capnp::any_pointer;
    use capnp::traits::{Owned, HasTypeId};
    use mozaic::chat_capnp;
    use mozaic::errors::*;

    use cursive::{Cursive, CbSink};
    use cursive::views::{TextView, EditView};
    use futures::{Future, Async, Poll, Stream};
    use futures::sync::mpsc;
    use mozaic::client::runtime::RuntimeState;
    use mozaic::messaging::types::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct HandlerState {
        cb_sink: CbSink,
        runtime: Arc<Mutex<RuntimeState>>,
    }

    /// ? Reads all messages from the RuntimeState and handles them.
    /// ? It also creates new messages that are sent to the RuntimeState
    pub struct RuntimeWorker {
        msg_chan: mpsc::UnboundedReceiver<Message>,
        handler_core: HandlerCore<HandlerState>,
    }

    impl RuntimeWorker {
        pub fn spawn(
            rx: mpsc::UnboundedReceiver<Message>,
            cb_sink: CbSink,
            runtime: Arc<Mutex<RuntimeState>>
        ) {
                let mut worker = RuntimeWorker {
                    msg_chan: rx,
                    handler_core: HandlerCore::new(
                        HandlerState {
                            cb_sink,
                            runtime,
                        }
                    )
                };

                let _id = <chat_capnp::connect_to_gui::Owned as Owned>::Reader::type_id();
                worker.handler_core.on(
                    chat_capnp::connect_to_gui::Owned,
                    FnHandler::new(rt_connect_to_gui),
                );

                let _id = <chat_capnp::chat_message::Owned as Owned>::Reader::type_id();
                worker.handler_core.on(
                    chat_capnp::chat_message::Owned,
                    FnHandler::new(rt_display_chat_message),
                );

                tokio::spawn(worker);
        }

        fn handle_message(&mut self, msg: Message) {
            self.handler_core.handle(&msg)
                .expect("message handling failed");
        }
    }

    impl Future for RuntimeWorker {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            loop {
                match try_ready!(self.msg_chan.poll()) {
                    None => return Ok(Async::Ready(())),
                    Some(msg) => self.handle_message(msg),
                }
            }
        }
    }

    fn rt_connect_to_gui(
        ctx: &mut RtHandlerCtx<HandlerState>,
        _: chat_capnp::connect_to_gui::Reader
    ) -> Result<()>
    {
        let reactor_id = ctx.sender_id.clone();
        let runtime = ctx.state.runtime.clone();

        ctx.state.cb_sink.send(Box::new(|cursive: &mut Cursive| {
            cursive.call_on_id("input", |view: &mut EditView| {
                view.set_on_submit(move |cursive, input_text| {
                    runtime.lock().unwrap().send_message(
                        &reactor_id,
                        chat_capnp::user_input::Owned,
                        |b| {
                            let mut input: chat_capnp::user_input::Builder = b.init_as();
                            input.set_text(input_text);
                        }).consume();
                    cursive.call_on_id("input", |view: &mut EditView| {
                        view.set_content("");
                    });
                })
            });
        })).unwrap();

        return Ok(());
    }

    fn rt_display_chat_message(
        ctx: &mut RtHandlerCtx<HandlerState>,
        msg: chat_capnp::chat_message::Reader
    ) -> Result<()>
    {
        let message = msg.get_message()?.to_string();
        let user = msg.get_user()?.to_string();
        ctx.state.cb_sink.send(Box::new(|cursive: &mut Cursive| {
            cursive.call_on_id("messages", |view: &mut TextView| {
                view.append(user);
                view.append(": ");
                view.append(message);
                view.append("\n");
            });
        })).unwrap();
        return Ok(());
    }

    struct RtHandlerCtx<'a, S> {
        pub sender_id: &'a ReactorId,
        pub state: &'a mut S,
    }


    type RtMsgHandler<S> = Box<
        dyn for<'a>
            Handler<'a,
                RtHandlerCtx<'a, S>,
                any_pointer::Owned, Output=(), Error=errors::Error>
    >;

    pub struct HandlerCore<S> {
        state: S,
        handlers: HashMap<u64, RtMsgHandler<S>>,
    }

    impl<S> HandlerCore<S> {
        pub fn new(state: S) -> Self {
            HandlerCore {
                state,
                handlers: HashMap::new(),
            }
        }

        fn on<M, H>(&mut self, _m: M, h: H)
            where M: for<'a> Owned<'a> + Send + 'static,
                <M as Owned<'static>>::Reader: HasTypeId,
                H: 'static + for <'a> Handler<'a, RtHandlerCtx<'a, S>, M, Output=(), Error=errors::Error>
        {
            let boxed = Box::new(AnyPtrHandler::new(h));
            self.handlers.insert(
                <M as Owned<'static>>::Reader::type_id(),
                boxed,
            );
        }

        pub fn handle(&mut self, message: &Message)
            -> Result<()>
        {
            let reader = message.reader();
            let type_id = reader.get()?.get_type_id();

            if let Some(handler) = self.handlers.get(&type_id) {
                let sender_id: ReactorId = reader.get()?.get_sender()?.into();
                let mut ctx = RtHandlerCtx {
                    sender_id: &sender_id,
                    state: &mut self.state,
                };
                handler.handle(&mut ctx, reader.get()?.get_payload())?;
            }
            return Ok(());
        }
    }

}
