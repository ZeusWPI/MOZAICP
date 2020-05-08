use crate::generic::*;
use crate::modules::types::*;
use crate::util::request::*;

use std::any;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct InitConnect(pub PlayerId, pub String);

pub struct Aggregator {
    host_id: ReactorID,
    cm_id: ReactorID,
    clients: HashMap<PlayerId, ReactorID>,

    init_connected: HashMap<PlayerId, Option<String>>,
    current_requests: HashMap<UUID, HashMap<PlayerId, Option<Connect>>>,
}

impl Aggregator {
    pub fn params(
        host_id: ReactorID,
        cm_id: ReactorID,
        clients: HashMap<PlayerId, ReactorID>,
    ) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Aggregator {
            host_id,
            cm_id,
            init_connected: clients.keys().map(|x| (*x, None)).collect(),
            clients,
            current_requests: HashMap::new(),
        })
        .handler(FunctionHandler::from(Self::handle_state_req))
        .handler(FunctionHandler::from(Self::handle_conn))
        .handler(FunctionHandler::from(Self::handle_init_connect))
        .handler(FunctionHandler::from(Self::handle_host_msg))
        .handler(FunctionHandler::from(
            Self::handle_register_client_controller,
        ))
        .handler(FunctionHandler::from(
            Self::handle_disconnect_client_controller,
        ))
    }

    fn handle_init_connect(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        con: &InitConnect,
    ) {
        self.init_connected
            .get_mut(&con.0)
            .map(|x| *x = Some(con.1.clone()));

        if self.init_connected.values().all(|x| x.is_some()) {
            // Send start
            let players: Vec<(PlayerId, String)> = self
                .init_connected
                .iter()
                .map(|(id, name)| (*id, name.clone().unwrap()))
                .collect();
            handle.send_internal(Start { players }, TargetReactor::Link(self.host_id));
        }
    }

    fn handle_conn(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        Res(uuid, conn): &Res<Connect>,
    ) {
        let (id, res) = match conn {
            Connect::Connected(id, name) => (id, Connect::Connected(*id, name.clone())),
            Connect::Reconnecting(id, name) => (id, Connect::Reconnecting(*id, name.clone())),
            Connect::Waiting(id, key) => (id, Connect::Waiting(*id, *key)),
            _ => panic!("Wrong connection response"),
        };

        let mut done = false;
        if let Some(requests) = self.current_requests.get_mut(uuid) {
            requests.insert(*id, Some(res));

            if requests.values().all(Option::is_some) {
                handle.send_internal(
                    Res(
                        *uuid,
                        State::Response(requests.values().cloned().filter_map(|x| x).collect()),
                    ),
                    TargetReactor::Link(self.host_id),
                );
                done = true;
            }
        }

        if done {
            self.current_requests.remove(uuid);
        }
    }

    fn handle_state_req(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        req: &Req<State>,
    ) {
        let waiting = self.clients.keys().map(|id| (*id, None)).collect();
        self.current_requests.insert(req.0, waiting);

        for id in self.clients.values() {
            handle.send_internal(Req(req.0, Connect::Request), TargetReactor::Link(*id));
        }
    }

    fn handle_host_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, msg: &HostMsg) {
        let target = match msg {
            HostMsg::Data(_, id) => id.clone(),
            HostMsg::Kick(id) => Some(*id),
        };

        if let Some(player_id) = target {
            if let Some(target) = self.clients.get(&player_id) {
                handle.send_internal(msg.clone(), TargetReactor::Link(*target));
            }
        } else {
            handle.send_internal(msg.clone(), TargetReactor::Links);
        }
    }

    fn handle_register_client_controller(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        NewClientController(player, reactor): &NewClientController,
    ) {
        self.clients.insert(*player, *reactor);
        let p = *player;
        handle.open_link(
            *reactor,
            ClientLink::params(self.host_id).closer(move |_state, handle| {
                handle.send_internal(ClientControllerDisconnect(p), TargetReactor::Reactor)
            }),
            false,
        )
    }

    fn handle_disconnect_client_controller(
        &mut self,
        _handle: &mut ReactorHandle<any::TypeId, Message>,
        ClientControllerDisconnect(player): &ClientControllerDisconnect,
    ) {
        self.clients.remove(player);
    }
}

impl ReactorState<any::TypeId, Message> for Aggregator {
    const NAME: &'static str = "Aggregator";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        for client_id in self.clients.values() {
            handle.open_link(*client_id, ClientLink::params(self.host_id), false);
        }
        handle.open_link(self.host_id, HostLink::params(), true);
        handle.open_link(self.cm_id, CMLink::params(), true);
    }
}

struct ClientLink;
impl ClientLink {
    fn params(host_id: ReactorID) -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self)
            .internal_handler(FunctionHandler::from(i_to_e::<Self, HostMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<Self, Req<Connect>>()))
            .external_handler(FunctionHandler::from(e_to_i::<Self, PlayerMsg>(
                TargetReactor::Link(host_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<Self, ClientStateUpdate>(
                TargetReactor::Link(host_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<Self, Res<Connect>>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<Self, InitConnect>(
                TargetReactor::Reactor,
            )))
    }
}

struct HostLink;
impl HostLink {
    fn params() -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self)
            .internal_handler(FunctionHandler::from(i_to_e::<Self, ClientStateUpdate>()))
            .internal_handler(FunctionHandler::from(i_to_e::<Self, PlayerMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<Self, Start>()))
            .internal_handler(FunctionHandler::from(i_to_e::<Self, Res<State>>()))
            .external_handler(FunctionHandler::from(e_to_i::<Self, HostMsg>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<Self, Req<State>>(
                TargetReactor::Reactor,
            )))
    }
}

struct CMLink;
impl CMLink {
    fn params() -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self).external_handler(FunctionHandler::from(e_to_i::<
            Self,
            NewClientController,
        >(
            TargetReactor::Reactor
        )))
    }
}
