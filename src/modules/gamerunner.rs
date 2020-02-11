use crate::modules::types::{PlayerMsg, HostMsg};
use crate::generic::*;

use std::any;

pub trait GameController {
    fn step<'a>(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg>;
}
pub type GameBox = Box<dyn GameController + Send>;


pub struct GameRunner {
    clients_id: ReactorID,
    game: GameBox,
}

impl GameRunner {
    pub fn params<G: GameController + Send + 'static>(clients_id: ReactorID, game: G) -> CoreParams<Self, any::TypeId, Message> {
        let me = Self {
            clients_id,
            game: Box::new(game),
        };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_client_msg))
            .handler(FunctionHandler::from(Self::handle_client_msgs))
    }

    fn handle_client_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, msg: &PlayerMsg) {
        for msg in self.game.step(vec![msg.clone()]) {
            handle.send_internal(msg, TargetReactor::Links);
        }
    }

    fn handle_client_msgs(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, msgs: &Vec<PlayerMsg>) {
        for msg in self.game.step(msgs.clone()) {
            handle.send_internal(msg, TargetReactor::Links);
        }
    }
}

impl ReactorState<any::TypeId, Message> for GameRunner {
    const NAME: &'static str = "Game";
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let client_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), HostMsg>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), PlayerMsg>(TargetReactor::Reactor)))
            .external_handler(FunctionHandler::from(e_to_i::<(), Vec<PlayerMsg>>(TargetReactor::Reactor)));
        handle.open_link(self.clients_id, client_params, true);
    }
}
