

use messaging::reactor::*;
use messaging::types::*;
use errors::{Result};
use core_capnp::{initialize, set_timeout};
use client_capnp::{from_client, to_client, host_message, client_step, client_turn};

use modules::util::{PlayerId};

#[derive(Debug, Clone)]
pub enum Turn<'a> {
    Timeout,
    Action(&'a [u8]),
}

pub type PlayerTurn<'a> = (PlayerId, Turn<'a>);

#[derive(Debug, Clone)]
pub enum Update {
    Global(Vec<u8>),
    Player(PlayerId, Vec<u8>),
}

pub trait GameController {
    fn step<'a>(&mut self, turns: Vec<PlayerTurn<'a>>) -> Vec<Update>;
}

pub struct GameReactor {
    game_controller: Box<dyn GameController>,
    clients_id: ReactorId,
}

impl GameReactor {
    pub fn params<C: Ctx>(clients_id: ReactorId, game_controller: Box<dyn GameController>) -> CoreParams<Self, C> {
        let me = Self {
            clients_id, game_controller
        };

        let mut params = CoreParams::new(me);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(from_client::Owned, CtxHandler::new(Self::handle_turn));
        params.handler(client_step::Owned, CtxHandler::new(Self::handle_turns));

        params
    }

    /// Initialize by opening a link to the ip endpoint
    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        handle.open_link(ClientLink::params(self.clients_id.clone()))?;
        Ok(())
    }

    fn handle_turn<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: from_client::Reader,
    ) -> Result<()>
    {
        let id: PlayerId = msg.get_client_id().into();
        let data = msg.get_data()?;

        self.step(handle, vec![(id, Turn::Action(data))])?;

        Ok(())
    }

    fn handle_turns<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msgs: client_step::Reader,
    ) -> Result<()>
    {
        let mut turns = Vec::new();
        for turn in msgs.get_data()?.iter() {
            let id: PlayerId = turn.get_client_id().into();
            let t = match turn.which()? {
                client_turn::Which::Turn(d) => Turn::Action(d?),
                client_turn::Which::Timeout(_) => Turn::Timeout,
            };
            turns.push((id, t));
        }
        self.step(handle, turns)?;

        Ok(())
    }

    fn step<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msgs: Vec<PlayerTurn>,
    ) -> Result<()> {
        for update in self.game_controller.step(msgs).iter() {
            match update {
                Update::Global(data) => {
                    let mut host_message = MsgBuffer::<host_message::Owned>::new();
                    host_message.build(|b| {
                        b.set_data(data);
                    });
                    handle.send_internal(host_message)?;
                },
                Update::Player(id, data) => {
                    let mut to_client = MsgBuffer::<to_client::Owned>::new();
                    to_client.build(|b| {
                        b.set_data(data);
                        b.set_client_id(**id);
                    });
                    handle.send_internal(to_client)?;
                },
            }
        }
        handle.send_internal(MsgBuffer::<set_timeout::Owned>::new())?;
        Ok(())
    }
}

struct ClientLink;
impl ClientLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ClientLink);
        params.internal_handler(host_message::Owned, CtxHandler::new(host_message::i_to_e));
        params.internal_handler(to_client::Owned, CtxHandler::new(to_client::i_to_e));
        params.internal_handler(set_timeout::Owned, CtxHandler::new(set_timeout::i_to_e));

        params.external_handler(from_client::Owned, CtxHandler::new(from_client::e_to_i));
        params.external_handler(client_step::Owned, CtxHandler::new(client_step::e_to_i));

        return params;
    }
}
