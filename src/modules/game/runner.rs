use crate::generic::*;
use crate::modules::types::{HostMsg, PlayerMsg, Start};

use super::request::*;
use super::GameBox;

use std::any;

pub struct Runner {
    clients_id: ReactorID,
    gm_id: ReactorID,
    game: GameBox,
}

impl Runner {
    pub fn params(
        clients_id: ReactorID,
        gm_id: ReactorID,
        game: GameBox,
    ) -> CoreParams<Self, any::TypeId, Message> {
        let me = Self {
            clients_id,
            gm_id,
            game,
        };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_client_msg))
            .handler(FunctionHandler::from(Self::handle_client_msgs))
            .handler(FunctionHandler::from(Self::handle_kill))
            .handler(FunctionHandler::from(Self::handle_start))
    }

    fn handle_start(&mut self, _handle: &mut ReactorHandle<any::TypeId, Message>, _msg: &Start) {
        self.game.start();
    }

    fn handle_client_msg(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        msg: &PlayerMsg,
    ) {
        info!("Game step");
        for msg in self.game.step(vec![msg.clone()]) {
            handle.send_internal(msg, TargetReactor::Links);
        }
        if self.game.is_done() {
            info!("Game step");
            handle.close();
        }
    }

    fn handle_client_msgs(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        msgs: &Vec<PlayerMsg>,
    ) {
        for msg in self.game.step(msgs.clone()) {
            handle.send_internal(msg, TargetReactor::Links);
        }
    }

    fn handle_kill(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, req: &Req<Kill>) {
        handle.send_internal(Res::<Kill>::default(req.0), TargetReactor::Link(self.gm_id));
        handle.close();
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
            .external_handler(FunctionHandler::from(e_to_i::<(), Req<State>>(
                TargetReactor::Link(self.clients_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Req<Kill>>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.gm_id, gm_link_params, false);
    }
}
