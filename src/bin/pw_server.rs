use std::env;

extern crate sodiumoxide;
extern crate serde_json;
extern crate tokio;
extern crate futures;
extern crate mozaic;
extern crate rand;
extern crate capnp;

use std::net::SocketAddr;
use mozaic::core_capnp::{initialize, actor_joined};
use mozaic::messaging::types::*;
use mozaic::messaging::reactor::*;
use mozaic::server::run_server;

pub mod pw {
    include!(concat!(env!("OUT_DIR"), "/planetwars_capnp.rs"));
}

// Load the config and start the game.
fn main() {
    run(env::args().collect());
}


struct PwRuntime {
    runtime_id: ReactorId,
}

impl PwRuntime {
    fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::welcome));
        params.handler(
            pw::pw_move::Owned,
            CtxHandler::new(Self::handle_turn)
        );
        return params;
    }

    fn initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        let link = ClientServerLink { };
        handle.open_link(link.params(self.runtime_id.clone()));
        return Ok(());
    }

    fn welcome<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), capnp::Error>
    {
        let id: ReactorId = r.get_id()?.into();
        println!("welcoming {:?}", id);
        let link = ClientServerLink { };
        handle.open_link(link.params(id));
        return Ok(());
    }

    fn handle_turn<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: pw::pw_move::Reader,
    ) -> Result<(), capnp::Error>
    {
        println!("{}: {}", r.get_user_id(), r.get_move()?);
        return Ok(());
    }
}

struct ClientServerLink {
}

impl ClientServerLink {
    fn params<C: Ctx>(self, client_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(client_id, self);

        params.internal_handler(
            pw::pw_state::Owned,
            CtxHandler::new(Self::send_state),
        );

        params.external_handler(
            pw::pw_move::Owned,
            CtxHandler::new(Self::recv_turn),
        );

        return params;
    }

    fn send_state<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: pw::pw_state::Reader,
    ) -> Result<(), capnp::Error>
    {
        let turn = message.get_turn();
        let state = message.get_state()?;

        let mut pw_state = MsgBuffer::<pw::pw_state::Owned>::new();
        pw_state.build(|b| {
            b.set_turn(turn);
            b.set_state(state);
        });
        handle.send_message(pw_state);

        return Ok(());
    }

    fn recv_turn<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        message: pw::pw_move::Reader,
    ) -> Result<(), capnp::Error>
    {
        let turn = message.get_move()?;
        let client_id = message.get_user_id();
        println!("Got move from {}", client_id);

        let mut turn_with_id = MsgBuffer::<pw::pw_move::Owned>::new();
        turn_with_id.build(|b| {
            b.set_move(turn);
            b.set_user_id(client_id);
        });

        handle.send_internal(turn_with_id);

        return Ok(());
    }
}

pub fn run(_args : Vec<String>) {

    let addr = "127.0.0.1:9142".parse::<SocketAddr>().unwrap();

    run_server(addr, |runtime_id| {
        let w = PwRuntime { runtime_id };
        return w.params();
    });
}
