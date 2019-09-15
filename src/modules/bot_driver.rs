use messaging::reactor::*;
use messaging::types::*;
use errors::{self, Consumable};
use core_capnp::{initialize};

use mozaic_cmd_capnp::{bot_input, bot_return};

use std::process::{Command, Stdio};
use std::io::{Write};
use std::io::BufReader;

use server::runtime::BrokerHandle;

use futures::{Future, Stream};

use tokio_process::{ChildStdin, ChildStdout, CommandExt};

enum Bot {
    ToSpawn(Vec<String>),
    Spawned(ChildStdin),
}

/// Reactor to handle cmd input
pub struct BotReactor {
    foreign_id: ReactorId,
    broker: BrokerHandle,
    bot: Bot,
}

impl BotReactor {
    pub fn new(broker: BrokerHandle, foreign_id: ReactorId, bot_cmd: Vec<String>) -> Self {

        Self {
            broker, foreign_id, bot: Bot::ToSpawn(bot_cmd)
        }

    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(bot_return::Owned, CtxHandler::new(Self::handle_return));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), errors::Error>
    {
        let args = match &self.bot {
            Bot::ToSpawn(v) => v,
            _ => return Ok(())
        };

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);

        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let mut bot = cmd.spawn_async()
            .expect("Couldn't spawn bot");

        let stdout = bot.stdout().take()
            .expect("child did not have a handle to stdout");

        let stdin = bot.stdin().take()
            .expect("child did not have a handle to stdin");

        let child_future = bot
            .map(|status| println!("child status was: {}", status))
            .map_err(|e| panic!("error while running child: {}", e));

        tokio::spawn(child_future);


        self.bot = Bot::Spawned(stdin);

        handle.open_link(BotLink::params(handle.id().clone()))?;
        handle.open_link(ForeignLink::params(self.foreign_id.clone()))?;

        setup_async_bot_stdout(self.broker.clone(), handle.id().clone(), stdout);
        Ok(())
    }

    fn handle_return<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        r: bot_return::Reader,
    ) -> Result<(), errors::Error> {

        if let Bot::Spawned(ref mut stdin) = self.bot {
            let msg = r.get_message()?;
            stdin.write_all(msg.as_bytes())?;
            stdin.flush()?;
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
        params.internal_handler(
            bot_input::Owned,
            CtxHandler::new(bot_input::i_to_e),
        );

        params.external_handler(
            bot_return::Owned,
            CtxHandler::new(bot_return::e_to_i)
        );

        return params;
    }
}

/// Link from the bot to the reactor, only handling incoming messages
struct BotLink;
impl BotLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);
        params.external_handler(bot_input::Owned, CtxHandler::new(bot_input::e_to_i), );
        return params;
    }
}

fn setup_async_bot_stdout(mut broker: BrokerHandle, id: ReactorId, stdout: ChildStdout) {
    tokio::spawn(
        tokio::io::lines(BufReader::new(stdout))
            // Convert any io::Error into a failure::Error for better flexibility
            .map_err(|e| eprintln!("{:?}", e))
            .for_each(move |input| {
                    broker.send_message(
                    &id,
                    &id,
                    bot_input::Owned,
                    move |b| {
                        let mut msg: bot_input::Builder = b.init_as();
                        msg.set_input(&input);
                    }
                ).display();
                Ok(())
            })
    );
}
