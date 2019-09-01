#![feature(slice_patterns)]

extern crate mozaic;
extern crate tokio;
extern crate futures;
extern crate rand;

use mozaic::{modules};
use mozaic::mozaic_cmd_capnp::{cmd_input, cmd_return};
use mozaic::core_capnp::initialize;
use mozaic::match_control_capnp::{start_game};
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::server::runtime::{Broker, BrokerHandle};

use rand::Rng;

use std::collections::HashMap;

static HELP: &'static str = "\
Please use one of the following commands:

start [turn_count] [path_to_map_file]   To start a new game instance.
status                                  Get the status update of all game instances.
status [id]                             Get status of game instance.
kill [id]                               Kill game instance.
help                                    Print this help message.
";


fn main() {
    let stupid_id: ReactorId = rand::thread_rng().gen();
    let cmd_id: ReactorId = rand::thread_rng().gen();


    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new();

        broker.spawn(stupid_id.clone(), Reactor::params(cmd_id.clone(), broker.clone()), "Main");
        broker.spawn(cmd_id.clone(), modules::CmdReactor::new(broker.clone(), stupid_id.clone()).params(), "Cmd");

        return Ok(());
    }));
}


/// The main starting reactor
struct Reactor {
    cmd_id: ReactorId,
    broker: BrokerHandle,
    games: HashMap<ReactorId, i32>,
}

impl Reactor {
    fn new(cmd_id: ReactorId, broker: BrokerHandle) -> Self {
        Reactor {
            cmd_id, broker,
            games: HashMap::new(),
        }
    }

    fn params<C: Ctx>(cmd_id: ReactorId, broker: BrokerHandle) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(Reactor::new(cmd_id, broker));
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(start_game::Owned, CtxHandler::new(Self::start_game));
        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        // open link with command line
        handle.open_link(CmdLink.params(self.cmd_id.clone()));

        // handle.open_link(modules::LogLink::params(modules::logger_id()));
        modules::log_reactor(handle, "Starting!!");
        Ok(())
    }

    fn start_game<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: start_game::Reader,
    ) -> Result<(), capnp::Error> {
        modules::log_reactor(handle, "Creating new game!!");

        Ok(())
    }

}

/// Listen for command line messages and handle them.
struct CmdLink;

impl CmdLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            cmd_input::Owned,
            CtxHandler::new(Self::e_handle_cmd),
        );

        params.internal_handler(
            cmd_return::Owned,
            CtxHandler::new(Self::i_handle_return),
        );

        return params;
    }

    fn e_handle_cmd<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: cmd_input::Reader,
    ) -> Result<(), capnp::Error> {

        // TODO: make fancy
        // Parse the input
        let split: Vec<&str> = r.get_input()?.split(" ").collect();
        let error_msg = match split[..] {
            ["start", turn_count, path, ..] => {
                if let Ok(turn_count) = turn_count.parse() {
                    let mut joined = MsgBuffer::<start_game::Owned>::new();

                    joined.build(|b| {
                        b.set_map_path(path);
                        b.set_max_turns(turn_count);
                    });

                    handle.send_internal(joined);

                    return Ok(());
                }
                "That is not how you start a game...\n"
            },
            _ => {
                ""
            },
        };

        let msg = error_msg.to_string() + HELP;

        let mut joined = MsgBuffer::<cmd_return::Owned>::new();
        joined.build(|b| b.set_message(&msg));
        handle.send_message(joined);

        Ok(())
    }

    fn i_handle_return<C: Ctx> (
        &mut self,
        handle: &mut LinkHandle<C>,
        r: cmd_return::Reader,
    ) -> Result<(), capnp::Error> {
        let msg = r.get_message()?;

        let mut joined = MsgBuffer::<cmd_return::Owned>::new();
        joined.build(|b| b.set_message(msg));
        handle.send_message(joined);

        Ok(())
    }
}
