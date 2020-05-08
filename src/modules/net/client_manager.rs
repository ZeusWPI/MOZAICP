use super::ClientControllerBuilder;
use crate::generic::*;
use crate::modules::net::types::*;
use crate::modules::types::*;

use std::sync::{Arc, Mutex};

use futures::future::Future;

use std::any;
use std::collections::HashMap;
use std::pin::Pin;

pub type SpawnCC =
    Arc<Box<dyn ClientControllerBuilder<any::TypeId, Message> + 'static + Send + Sync>>;

#[derive(Clone)]
pub struct RegisterGame {
    pub game: u64,
    pub ag_id: ReactorID,
    pub players: HashMap<u64, (PlayerId, ReactorID)>,
    pub free_client: Option<(u64, SpawnCC)>,
}

#[derive(Clone, Debug)]
pub struct PlayerUUIDs {
    game: u64,
    ids: Vec<u64>,
}

pub type BoxSpawnPlayer = Arc<Mutex<Option<SpawnPlayer>>>;

pub struct SpawnPlayer {
    pub register: Register,
    pub builder: Box<
        dyn FnOnce(
                ReactorID,
                SenderHandle<any::TypeId, Message>,
            ) -> (
                Sender<any::TypeId, Message>,
                Pin<Box<dyn Future<Output = ()> + Send>>,
                &'static str,
            ) + Send
            + Sync
            + 'static,
    >,
}

impl SpawnPlayer {
    pub fn new<
        F: FnOnce(
                ReactorID,
                SenderHandle<any::TypeId, Message>,
            ) -> (
                Sender<any::TypeId, Message>,
                Pin<Box<dyn Future<Output = ()> + Send>>,
                &'static str,
            )
            + 'static
            + Send
            + Sync,
    >(
        register: Register,
        f: F,
    ) -> BoxSpawnPlayer {
        Arc::new(Mutex::new(Some(Self {
            register,
            builder: Box::new(f),
        })))
    }
}

#[derive(Clone)]
pub struct RegisterEndpoint(pub ReactorID);

pub struct ClientManager {
    clients: HashMap<u64, (PlayerId, ReactorID)>,
    extra_ccs: HashMap<u64, SpawnCC>,
    game_manager: ReactorID,
    endpoints: Vec<RegisterEndpoint>,
}

impl ClientManager {
    pub fn new(
        game_manager: ReactorID,
        endpoints: Vec<ReactorID>,
    ) -> CoreParams<Self, any::TypeId, Message> {
        let me = Self {
            game_manager,
            clients: HashMap::new(),
            extra_ccs: HashMap::new(),
            endpoints: endpoints.iter().map(|x| RegisterEndpoint(*x)).collect(),
        };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_spawn_game))
            .handler(FunctionHandler::from(Self::handle_cc_close))
            .handler(FunctionHandler::from(Self::handle_register_endpoint))
            .handler(FunctionHandler::from(Self::handle_player_regiser))
    }

    fn handle_spawn_game(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        cs: &RegisterGame,
    ) {
        self.clients.extend(cs.players.clone());

        for (_, cc) in cs.players.values() {
            let cc_params = LinkParams::new(())
                .internal_handler(FunctionHandler::from(i_to_e::<(), Accepted>()))
                .closer(|_, handle| {
                    handle.send_internal(*handle.target_id(), TargetReactor::Reactor);
                });
            handle.open_link(*cc, cc_params, false)
        }

        if let Some((id, spawn_cc)) = &cs.free_client {
            self.extra_ccs.insert(*id, spawn_cc.clone());
        }

        let agg_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), NewClientController>()));
        handle.open_link(cs.ag_id, agg_params, false);
    }

    fn handle_cc_close(&mut self, _: &mut ReactorHandle<any::TypeId, Message>, id: &ReactorID) {
        let orig_len = self.clients.len();

        if orig_len == 0 {
            error!("Tried to remove client controller, but there are none");
            return;
        }

        self.clients.retain(|_, (_, x)| x != id);

        if orig_len == self.clients.len() {
            error!(%id, "Tried to remove client controller, but client controller was not there");
        } else if orig_len - 1 == self.clients.len() {
            trace!(%id, "Successfully removed client controller");
        } else {
            error!(%id, "Remove more then one client controller!");
        }
    }

    fn handle_register_endpoint(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        reg: &RegisterEndpoint,
    ) {
        let ep_link_params =
            LinkParams::new(()).external_handler(FunctionHandler::from(
                e_to_i::<(), BoxSpawnPlayer>(TargetReactor::Reactor),
            ));
        handle.open_link(reg.0, ep_link_params, false);
    }

    fn handle_player_regiser(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        reg: &BoxSpawnPlayer,
    ) {
        let mut reg = reg.lock().unwrap();
        let reg = std::mem::replace(&mut *reg, None);

        if let Some(SpawnPlayer {
            register: Register { id, name },
            builder,
        }) = reg
        {
            let maybe_player = self.clients.get(&id).cloned().or_else(|| {
                if let Some(spawn_cc) = self.extra_ccs.get(&id) {
                    // No client controller found, but we have to build extra!
                    let self_id = *handle.id();
                    let (ket, player, reactor_id, host_id) =
                        spawn_cc.build(handle.into_spawner(), self_id);

                    handle.send_internal(
                        NewClientController(player, reactor_id),
                        TargetReactor::Link(host_id),
                    );

                    self.clients.insert(ket, (player, reactor_id));

                    let cc_params = LinkParams::new(())
                        .internal_handler(FunctionHandler::from(i_to_e::<(), Accepted>()))
                        .closer(|_, handle| {
                            handle.send_internal(*handle.target_id(), TargetReactor::Reactor);
                        });
                    handle.open_link(reactor_id, cc_params, false);

                    Some((player, reactor_id))
                } else {
                    None
                }
            });

            if let Some((player, cc)) = maybe_player {
                let id = ReactorID::rand();
                let (chan, fut, handler_name) = builder(id, handle.get(&cc));
                handle.open_reactor_like(id, chan, fut, handler_name);

                let accept = Accepted {
                    player: player,
                    name,
                    client_id: id,
                    contr_id: cc,
                };
                handle.send_internal(accept.clone(), TargetReactor::Link(cc));
            }
        }
    }
}

impl ReactorState<any::TypeId, Message> for ClientManager {
    const NAME: &'static str = "Client Manager";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        for reg in &self.endpoints {
            let ep_link_params =
                LinkParams::new(()).external_handler(FunctionHandler::from(e_to_i::<
                    (),
                    BoxSpawnPlayer,
                >(
                    TargetReactor::Reactor,
                )));
            handle.open_link(reg.0, ep_link_params, false);
        }

        let gm_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), PlayerUUIDs>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), RegisterGame>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), RegisterEndpoint>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.game_manager, gm_link_params, false);
    }
}
