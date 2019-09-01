#![feature(slice_patterns)]

extern crate mozaic;
extern crate sodiumoxide;
extern crate hex;
extern crate tokio;
#[macro_use]
extern crate futures;
extern crate prost;
extern crate rand;
extern crate cursive;
extern crate crossbeam_channel;
extern crate capnp;

use mozaic::modules::{reactors, links};
use mozaic::mozaic_cmd_capnp::{cmd_input, cmd_return};
use mozaic::core_capnp::initialize;
use mozaic::my_capnp;
use mozaic::match_control_capnp::{start_game};
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::server::runtime::{Broker, Runtime};

use rand::Rng;

static HELP: &'static str = "\
Please use one of the following commands:

start [turn_count] [path_to_map_file]   To start a new game instance.
status                                  Get the status update of all game instances.
status [id]                             Get status of game instance.
kill [id]                               Kill game instance.
help                                    Print this help message.
";


fn main() {
    let mut broker = Broker::new();
    let stupid_id: ReactorId = rand::thread_rng().gen();
    let cmd_id: ReactorId = rand::thread_rng().gen();


    tokio::run(futures::lazy(move || {
        // let cmd_reactor = reactors::CmdReactor::new(broker.clone(), stupid_id.clone());
        // broker.spawn(cmd_id.clone(), cmd_reactor.params());

        broker.spawn(stupid_id.clone(), Reactor::params(cmd_id.clone()));
        broker.spawn(cmd_id.clone(), reactors::CmdReactor::new(broker.clone(), stupid_id).params());
        return Ok(());
    }));
}

struct Reactor {
    cmd_id: ReactorId,
}

impl Reactor {
    fn params<C: Ctx>(cmd_id: ReactorId) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(Reactor { cmd_id });
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        // sending to the cmd
        handle.open_link(CmdLink.params(self.cmd_id.clone()));

        Ok(())
    }

}

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

                    handle.send_message(joined);

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
