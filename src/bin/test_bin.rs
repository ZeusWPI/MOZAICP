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
use mozaic::server::runtime::{Broker, BrokerHandle};

use futures::Future;

use rand::Rng;

fn main() {
    let mut broker = Broker::new();
    let stupid_id: ReactorId = rand::thread_rng().gen();

    tokio::run(futures::lazy(move || {
        // let cmd_reactor = reactors::CmdReactor::new(broker.clone(), stupid_id.clone());
        // broker.spawn(cmd_id.clone(), cmd_reactor.params());
        broker.spawn(stupid_id.clone(), Reactor::params(broker.clone(), broker.get_runtime_id().clone()));

        return Ok(());
    }));
}

struct Reactor {
    broker: BrokerHandle,
    greeter_id: ReactorId,
}

impl Reactor {
    fn params<C: Ctx>(broker: BrokerHandle, greeter_id: ReactorId) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(Reactor { broker, greeter_id });
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

        let cmd_reactor = reactors::CmdReactor::new(self.broker.clone(), handle.id().clone());
        let cmd_id = handle.spawn(cmd_reactor.params());

        // receiving from the runtime
        let cmdLink = links::CommandLink { name: "mains linking to runtime"};
        handle.open_link(cmdLink.params(self.greeter_id.clone(), true));

        // sending to the cmd
        let cmdLink = links::CommandLink {name: "mains linking to cmd_id"};
        handle.open_link(cmdLink.params(cmd_id, false));

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
