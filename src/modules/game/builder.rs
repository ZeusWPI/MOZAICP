use super::{Controller, Runner};

use crate::generic::*;
use crate::modules::net::{
    client_controller::{self, spawner},
    SpawnCC,
};
use crate::modules::types::*;
use crate::modules::*;

use std::any;
use std::collections::HashMap;

pub type BoxedBuilder = Box<
    dyn FnOnce(
            BrokerHandle<any::TypeId, Message>,
            ReactorID,
            ReactorID,
            ReactorID,
            u64,
        ) -> (
            ReactorID,
            ReactorID,
            HashMap<u64, (PlayerId, ReactorID)>,
            Option<(u64, SpawnCC)>,
        ) + Send,
>;

pub struct Builder<G, S: spawner::IntoSpawner<any::TypeId, Message>> {
    steplock: Option<StepLock>,
    players: Vec<PlayerId>,
    game: G,
    free_client: Option<(u64, S)>,
}

impl<G: Clone, S: Clone + spawner::IntoSpawner<any::TypeId, Message>> Clone for Builder<G, S> {
    fn clone(&self) -> Self {
        Self {
            steplock: self.steplock.clone(),
            players: self.players.clone(),
            game: self.game.clone(),
            free_client: self.free_client.clone(),
        }
    }
}

impl<
        G: Controller + Send + 'static,
        S: spawner::IntoSpawner<any::TypeId, Message> + Send + 'static,
    > Builder<G, S>
{
    /// When called, you might want to wait a little
    /// It is a known but that not all connections are established
    /// right away.
    /// 100 ms should be more than enough, for most hardware.
    pub fn new(players: Vec<PlayerId>, game: G) -> Self {
        Self {
            players,
            steplock: None,
            game,
            free_client: None,
        }
    }

    pub fn with_step_lock(mut self, lock: StepLock) -> Self {
        self.steplock = Some(lock);
        self
    }

    pub fn with_free_client(mut self, id: u64, spawner: S) -> Self {
        self.free_client = Some((id, spawner));
        self
    }

    fn build(
        self,
        broker: BrokerHandle<any::TypeId, Message>,
        gm_id: ReactorID,
        cm_id: ReactorID,
        logger_id: ReactorID,
        id: u64,
    ) -> (
        ReactorID,
        ReactorID,
        HashMap<u64, (PlayerId, ReactorID)>,
        Option<(u64, SpawnCC)>,
    ) {
        let game_id = ReactorID::rand();
        let step_id = ReactorID::rand();
        let agg_id = ReactorID::rand();

        let players: HashMap<u64, (PlayerId, ReactorID)> = self
            .players
            .iter()
            .map(|&x| {
                let key = rand::random();
                let params =
                    client_controller::ClientController::params(cm_id, agg_id, x, key, true);
                let id = broker.spawn(params, None);
                (key, (x, id))
            })
            .collect();

        let game = Runner::params(
            if self.steplock.is_some() {
                step_id
            } else {
                agg_id
            },
            gm_id,
            logger_id,
            Box::new(self.game),
            id,
        );

        let agg = Aggregator::params(
            if self.steplock.is_some() {
                step_id
            } else {
                game_id
            },
            cm_id,
            players.values().cloned().collect(),
        );

        if let Some(lock) = self.steplock.map(|lock| lock.params(game_id, agg_id)) {
            broker.spawn(lock, Some(step_id));
        }
        broker.spawn(game, Some(game_id));

        broker.spawn(agg, Some(agg_id));

        let free_client = self
            .free_client
            .map(|(id, fc)| (id, fc.into_spawner(agg_id)));

        (game_id, agg_id, players, free_client)
    }
}

impl<
        G: Controller + Send + 'static,
        S: spawner::IntoSpawner<any::TypeId, Message> + Send + 'static,
    > Into<BoxedBuilder> for Builder<G, S>
{
    fn into(self) -> BoxedBuilder {
        Box::new(|broker, gm_id, cm_id, logger_id, id| {
            self.build(broker, gm_id, cm_id, logger_id, id)
        })
    }
}
