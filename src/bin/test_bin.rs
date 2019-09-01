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
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::server::runtime::{Broker, BrokerHandle, Runtime};

use futures::Future;

use rand::Rng;

struct MyParser;

impl reactors::Parser<Runtime> for MyParser {

    fn parse(
        &mut self,
        input: &str,
        handle: &mut LinkHandle<Runtime>
    ) -> Result<Option<String>, capnp::Error> {
        println!("Trying to parse {}", input);
        Ok(Some(":/".to_string()))
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
