use crate::generic::*;
use crate::modules::types::{HostMsg, PlayerMsg, Start};

use super::request::*;
use super::GameBox;

use std::any;

use serde_json::Value;

pub struct Runner {
    clients_id: ReactorID,
    gm_id: ReactorID,
    logger_id: ReactorID,
    game: GameBox,
    game_id: u64,
}

impl Runner {
    pub fn params(
        clients_id: ReactorID,
        gm_id: ReactorID,
        logger_id: ReactorID,
        game: GameBox,
        game_id: u64,
    ) -> CoreParams<Self, any::TypeId, Message> {
        let me = Self {
            clients_id,
            gm_id,
            game,
            logger_id,
            game_id,
        };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_client_msg))
            .handler(FunctionHandler::from(Self::handle_client_msgs))
            .handler(FunctionHandler::from(Self::handle_kill))
            .handler(FunctionHandler::from(Self::handle_start))
    }

    fn handle_start(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, _msg: &Start) {
        for msg in self.game.start() {
            handle.send_internal(msg, TargetReactor::Links);
        }

        self.maybe_close(handle);
    }

    fn handle_client_msg(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        msg: &PlayerMsg,
    ) {
        for msg in self.game.step(vec![msg.clone()]) {
            handle.send_internal(msg, TargetReactor::Links);
        }

        self.maybe_close(handle);
    }

    fn handle_client_msgs(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        msgs: &Vec<PlayerMsg>,
    ) {
        for msg in self.game.step(msgs.clone()) {
            handle.send_internal(msg, TargetReactor::Links);
        }

        self.maybe_close(handle);
    }

    fn handle_kill(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, req: &Req<Kill>) {
        handle.send_internal(Res::<Kill>::default(req.0), TargetReactor::Link(self.gm_id));
        handle.close();
    }

    fn maybe_close(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, ) {
        if let Some(done) = self.game.is_done() {
            handle.send_internal((self.game_id, done.clone()), TargetReactor::Link(self.gm_id));
            handle.send_internal(done, TargetReactor::Link(self.logger_id));
            handle.close();
        }
    }
}

impl ReactorState<any::TypeId, Message> for Runner {
    const NAME: &'static str = "Game";
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let client_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), HostMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), Req<State>>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), PlayerMsg>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Start>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Vec<PlayerMsg>>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Res<State>>(
                TargetReactor::Link(self.gm_id),
            )));
        handle.open_link(self.clients_id, client_params, true);

        let gm_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), Res<State>>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), Res<Kill>>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), (u64, (String, Value))>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), Req<State>>(
                TargetReactor::Link(self.clients_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Req<Kill>>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.gm_id, gm_link_params, false);


        let logger_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), (String, Value)>()));
        handle.open_link(self.logger_id, logger_link_params, false);
    }
}
