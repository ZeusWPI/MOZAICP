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

struct MyParser;

impl reactors::Parser<Runtime> for MyParser {

    fn parse(
        &mut self,
        input: &str,
        handle: &mut LinkHandle<Runtime>
    ) -> Result<Option<String>, capnp::Error> {

        let split: Vec<&str> = input.split(" ").collect();
        Ok(match split[..] {
            ["start", turn_count, path, ..] => {
                let turn_count: u64 = match turn_count.parse() {
                    Ok(count) => count,
                    Err(_) => return Ok(Some(HELP.to_string())),
                };
                let mut joined = MsgBuffer::<start_game::Owned>::new();

                joined.build(|b| {
                    b.set_map_path(path);
                    b.set_max_turns(turn_count);
                });

                handle.send_message(joined);
                None
            },
            _ => {

                Some(HELP.to_string())
            },
        })
    }
}

fn main() {
    let mut broker = Broker::new();
    let stupid_id: ReactorId = rand::thread_rng().gen();
    let cmd_id: ReactorId = rand::thread_rng().gen();


    tokio::run(futures::lazy(move || {
        // let cmd_reactor = reactors::CmdReactor::new(broker.clone(), stupid_id.clone());
        // broker.spawn(cmd_id.clone(), cmd_reactor.params());

        broker.spawn(stupid_id.clone(), Reactor::params(cmd_id.clone()));
        broker.spawn(cmd_id.clone(), reactors::CmdReactor::new(broker.clone(), stupid_id, Box::new(MyParser)).params());
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

        params.handler(my_capnp::sent_message::Owned, CtxHandler::new(Self::handle_msg));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        // sending to the cmd
        let cmd_link = links::CommandLink {name: "mains linking to cmd_id"};
        handle.open_link(cmd_link.params(self.cmd_id.clone()));

        Ok(())
    }

    fn handle_msg<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: my_capnp::sent_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_message()?;

        println!("Reactor got msg {}", msg);

        Ok(())
    }
}
