use crate::generic::*;
use crate::modules::types::{HostMsg, PlayerMsg};

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
    pub fn params<G: GameController + Send + 'static>(
        clients_id: ReactorID,
        game: G,
    ) -> CoreParams<Self, any::TypeId, Message> {
        let me = Self {
            clients_id,
            game: Box::new(game),
        };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_client_msg))
            .handler(FunctionHandler::from(Self::handle_client_msgs))
    }

    fn handle_client_msg(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        msg: &PlayerMsg,
    ) {
        for msg in self.game.step(vec![msg.clone()]) {
            handle.send_internal(msg, TargetReactor::Links);
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
}

impl ReactorState<any::TypeId, Message> for GameRunner {
    const NAME: &'static str = "Game";
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let client_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), HostMsg>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), PlayerMsg>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), Vec<PlayerMsg>>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.clients_id, client_params, true);
    }
}

pub use builder::GameBuilder;
mod builder {
    use super::GameController;

    use crate::generic::*;
    use crate::modules::types::*;
    use crate::modules::*;

    use futures::executor::ThreadPool;

    use std::collections::HashMap;

    pub struct GameBuilder<G: GameController + Send + 'static> {
        steplock: Option<StepLock>,
        players: Vec<PlayerId>,
        game: G,
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

        pub async fn run(self, pool: ThreadPool) {
            let broker = BrokerHandle::new(pool.clone());
            let json_broker = BrokerHandle::new(pool.clone());

            let cc_ids: Vec<ReactorID> = self.players.iter().map(|_| ReactorID::rand()).collect();
            let player_map: HashMap<PlayerId, ReactorID> = self
                .players
                .iter()
                .cloned()
                .zip(cc_ids.iter().cloned())
                .collect();

            let game_id = ReactorID::rand();
            let step_id = ReactorID::rand();
            let agg_id = ReactorID::rand();
            let cm_id = ReactorID::rand();

            let lock = self.steplock.map(|lock| lock.params(game_id, agg_id));

            let game = GameRunner::params(
                if lock.is_some() {
                    step_id
                } else {
                    agg_id
                },
                self.game,
            );

            let agg = Aggregator::params(
                if lock.is_some() {
                    step_id
                } else {
                    game_id
                },
                player_map.clone(),
            );

            let cm = ConnectionManager::params(
                pool.clone(),
                "127.0.0.1:6666".parse().unwrap(),
                json_broker.clone(),
                player_map.clone(),
            );

            player_map
                .iter()
                .map(|(&id, &r_id)| {
                    ClientController::new(
                        r_id,
                        json_broker.clone(),
                        broker.clone(),
                        agg_id,
                        cm_id,
                        id,
                    )
                })
                .for_each(|cc| pool.spawn_ok(cc));
            
            if let Some(lock) = lock {
                broker.spawn(lock, Some(step_id));
            }

            join!(
                broker.spawn_with_handle(game, Some(game_id)).0,
                broker.spawn_with_handle(agg, Some(agg_id)).0,
                json_broker.spawn_with_handle(cm, Some(cm_id)).0,
            );
        }
    }
}
