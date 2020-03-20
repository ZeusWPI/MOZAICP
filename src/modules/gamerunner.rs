use crate::generic::*;
use crate::modules::types::{HostMsg, PlayerMsg};

use std::any;

pub trait GameController {
    fn step<'a>(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg>;
    fn is_done(&mut self) -> bool;
}
pub type GameBox = Box<dyn GameController + Send>;

pub struct GameRunner {
    clients_id: ReactorID,
    gm_id: ReactorID,
    game: GameBox,
}

impl GameRunner {
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

    fn handle_kill(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        req: &KillReq,
    ) {
        handle.send_internal(KillRes(req.0), TargetReactor::Link(self.gm_id));
        handle.close();
    }
}

use crate::modules::game_manager::types::*;
impl ReactorState<any::TypeId, Message> for GameRunner {
    const NAME: &'static str = "Game";
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let client_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), HostMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), StateReq>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), PlayerMsg>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Vec<PlayerMsg>>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), StateRes>(
                TargetReactor::Link(self.gm_id),
            )));
        handle.open_link(self.clients_id, client_params, true);

        let gm_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), StateRes>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), KillRes>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), StateReq>(TargetReactor::Link(self.clients_id))))
            .external_handler(FunctionHandler::from(e_to_i::<(), KillReq>(TargetReactor::Reactor)));
        handle.open_link(self.gm_id, gm_link_params, false);
    }
}

pub use builder::{GameBuilder, BoxedGameBuilder};
mod builder {
    use super::{GameController};

    use crate::generic::*;
    use crate::modules::net::client_controller;
    use crate::modules::types::*;
    use crate::modules::*;

    pub type BoxedGameBuilder = Box<dyn Send + FnOnce(BrokerHandle<any::TypeId, Message>, ReactorID, ReactorID) -> (ReactorID, Vec<(PlayerId, ReactorID)>)>;

    use std::any;

    pub struct GameBuilder<G> {
        steplock: Option<StepLock>,
        players: Vec<PlayerId>,
        game: G,
    }

    impl<G: Clone> Clone for GameBuilder<G> {
        fn clone(&self) -> Self {
            Self {
                steplock: self.steplock.clone(),
                players: self.players.clone(),
                game: self.game.clone(),
            }
        }
    }

    impl<G: GameController + Send + 'static> GameBuilder<G> {
        pub fn new(players: Vec<PlayerId>, game: G) -> Self {
            Self {
                players,
                steplock: None,
                game,
            }
        }
        pub fn with_step_lock(mut self, lock: StepLock) -> Self {
            self.steplock = Some(lock);
            self
        }


    }

    impl<G: GameController + Send + 'static> Into<BoxedGameBuilder> for GameBuilder<G> {
        fn into(
            self,
        ) -> BoxedGameBuilder {
            Box::new(
                |broker: BrokerHandle<any::TypeId, Message>, gm_id: ReactorID, cm_id: ReactorID,| {

                    let game_id = ReactorID::rand();
                    let step_id = ReactorID::rand();
                    let agg_id = ReactorID::rand();

                    let players: Vec<(PlayerId, ReactorID)> = self
                    .players
                    .iter()
                    .map(|&x| {
                        let params = client_controller::ClientController::params(cm_id, agg_id, x);
                        let id = broker.spawn(params, None);
                        (x, id)
                    })
                    .collect();

                    let game = GameRunner::params(
                        if self.steplock.is_some() {
                            step_id
                        } else {
                            agg_id
                        },
                        gm_id,
                        Box::new(self.game),
                    );
                    let agg = Aggregator::params(
                        if self.steplock.is_some() {
                            step_id
                        } else {
                            game_id
                        },
                        players.iter().cloned().collect(),
                    );

                    if let Some(lock) = self.steplock.map(|lock| lock.params(game_id, agg_id)) {
                        broker.spawn(lock, Some(step_id));
                    }
                    broker.spawn(game, Some(game_id));

                    broker.spawn(agg, Some(agg_id));

                    (game_id, players)
                }
            )
        }
    }
}
