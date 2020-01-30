use crate::core_capnp::initialize;
use crate::errors::*;
use crate::messaging::reactor::*;
use crate::messaging::types::*;

use crate::cmd_capnp::{bot_input, bot_return};

use std::io::BufReader;
use std::pin::Pin;
use std::process::{ChildStdout, Command, Stdio};

use crate::runtime::BrokerHandle;

use futures::task::{self, Poll, SpawnExt};
use futures::Future;
use std::io::BufRead;

enum Bot {
    ToSpawn(Vec<String>),
    Spawned(mpsc::Sender<Vec<u8>>),
}

/// Reactor to handle cmd input
pub struct BotReactor {
    foreign_id: ReactorId,
    broker: BrokerHandle,
    bot: Bot,
}

impl BotReactor {
    pub fn new(
        broker: BrokerHandle,
        foreign_id: ReactorId,
        bot_cmd: Vec<String>,
    ) -> Self {
        Self {
            broker,
            foreign_id,
            bot: Bot::ToSpawn(bot_cmd),
        }
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(bot_input::Owned, CtxHandler::new(Self::handle_return));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()> {
        let args = match &self.bot {
            Bot::ToSpawn(v) => v,
            _ => return Ok(()),
        };

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);

        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let bot = cmd.spawn().expect("Couldn't spawn bot");

        let stdout = bot.stdout.expect("child did not have a handle to stdout");

        let stdin = bot.stdin.expect("child did not have a handle to stdin");

        // let child_future = bot
        //     .map(|status| println!("child status was: {}", status))
        //     .map_err(|e| panic!("error while running child: {}", e));

        // tokio::spawn(child_future);

        let (fut, bot_sink) = BotSink::new(stdin);
        self.bot = Bot::Spawned(bot_sink);
        self.broker.pool().spawn(fut).expect("Couldn't spawn bot driver");

        handle.open_link(BotLink::params(handle.id().clone()))?;
        handle.open_link(ForeignLink::params(self.foreign_id.clone()))?;

        setup_async_bot_stdout(
            self.broker.clone(),
            handle.id().clone(),
            stdout,
        );
        Ok(())
    }

    fn handle_return<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        r: bot_input::Reader,
    ) -> Result<()> {
        if let Bot::Spawned(ref mut stdin) = self.bot {
            let msg = r.get_input()?;
            let mut msg = msg.to_vec();
            msg.push(b'\n');

            stdin.try_send(msg).expect("Damm it");
        }

        Ok(())
    }
}

/// Link from the cmd reactor to somewhere, sending through the cmd messages
/// Also listening for messages that have to return to the command line
struct ForeignLink;

impl ForeignLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);
        params.internal_handler(bot_return::Owned, CtxHandler::new(bot_return::i_to_e));

        params.external_handler(bot_input::Owned, CtxHandler::new(bot_input::e_to_i));

        return params;
    }
}

/// Link from the bot to the reactor, only handling incoming messages
struct BotLink;
impl BotLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);
        params.external_handler(bot_return::Owned, CtxHandler::new(bot_return::e_to_i));
        return params;
    }
}

fn setup_async_bot_stdout(mut broker: BrokerHandle, id: ReactorId, stdout: ChildStdout) {
    std::thread::spawn(move || {
        BufReader::new(stdout)
            .lines()
            // Convert any io::Error into a failure::Error for better flexibility
            // .map_err(|e| eprintln!("{:?}", e))
            .for_each(move |input| {
                if let Ok(input) = input {
                    broker
                        .send_message(&id, &id, bot_return::Owned, move |b| {
                            let mut msg: bot_return::Builder = b.init_as();
                            msg.set_message(&input.as_bytes());
                        })
                        .display();
                }
            });
    });
}

use std::collections::VecDeque;
use std::io::Write;
use tokio::sync::mpsc;

struct BotSink<A> {
    rx: mpsc::Receiver<Vec<u8>>,
    write: A,
    queue: VecDeque<Vec<u8>>,
}

impl<A> BotSink<A>
where
    A: Write + Send + 'static + Unpin,
{
    fn new(write: A) -> (impl Future<Output = ()>, mpsc::Sender<Vec<u8>>) {
        let (tx, rx) = mpsc::channel(10);

        let me = Self {
            rx,
            write,
            queue: VecDeque::new(),
        };

        (me, tx)
    }
}

impl<A> Future for BotSink<A>
where
    A: Write + Unpin,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, _ctx: &mut task::Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);
        loop {
            match this.rx.try_recv() {
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Closed) => return Poll::Ready(()),
                Ok(vec) => {
                    this.queue.push_back(vec);
                }
            }
        }

        // TODO make async again
        loop {
            match this.write.flush() {
                Err(_) => return Poll::Ready(()),
                _ => {}
            }

            if let Some(buffer) = this.queue.pop_front() {
                if this.write.write_all(&buffer).is_err() {
                    return Poll::Ready(());
                }
            }
        }
    }
}
