use crate::generic::*;
use crate::modules::types::*;

use std::any;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ConnectRes {
    Connected(PlayerId),
    Reconnecting(PlayerId),
    Waiting(PlayerId, u64),
}
#[derive(Debug, Clone)]
pub struct ConnectReq(pub u64);

pub struct Aggregator {
    host_id: ReactorID,
    clients: HashMap<PlayerId, ReactorID>,

    current_requests: HashMap<u64, HashMap<PlayerId, Option<ConnectRes>>>,
}

impl Aggregator {
    pub fn params(
        host_id: ReactorID,
        clients: HashMap<PlayerId, ReactorID>,
    ) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Aggregator {
            host_id,
            clients,
            current_requests: HashMap::new(),
        })
        .handler(FunctionHandler::from(Self::handle_state_req))
        .handler(FunctionHandler::from(Self::handle_conn))
    }

    fn handle_conn(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        (uuid, conn): &(u64, ConnectRes),
    ) {
        let (id, res) = match conn {
            ConnectRes::Connected(id) => (id, ConnectRes::Connected(*id)),
            ConnectRes::Reconnecting(id) => (id, ConnectRes::Reconnecting(*id)),
            ConnectRes::Waiting(id, key) => (id, ConnectRes::Waiting(*id, *key)),
        };

        let mut done = false;
        if let Some(requests) = self.current_requests.get_mut(uuid) {
            requests.insert(*id, Some(res));

            if requests.values().all(Option::is_some) {
                handle.send_internal(
                    StateRes(*uuid, format!("res: {:?}", requests.values().cloned().map(Option::unwrap).collect::<Vec<ConnectRes>>())),
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
        req: &StateReq,
    ) {
        let waiting = self.clients.keys().map(|id| (*id, None)).collect();
        self.current_requests.insert(req.0, waiting);

        for id in self.clients.values() {
            handle.send_internal(ConnectReq(req.0), TargetReactor::Link(*id));
        }
    }
}

impl ReactorState<any::TypeId, Message> for Aggregator {
    const NAME: &'static str = "Aggregator";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        for client_id in self.clients.values() {
            handle.open_link(*client_id, ClientLink::params(self.host_id), false);
        }
        handle.open_link(self.host_id, HostLink::params(self.clients.clone()), true);
    }
}

struct ClientLink;
impl ClientLink {
    fn params(host_id: ReactorID) -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self)
            .internal_handler(FunctionHandler::from(i_to_e::<Self, HostMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<Self, ConnectReq>()))
            .external_handler(FunctionHandler::from(e_to_i::<Self, PlayerMsg>(
                TargetReactor::Link(host_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<Self, (u64, ConnectRes)>(
                TargetReactor::Reactor,
            )))
    }
}

struct HostLink {
    clients: HashMap<PlayerId, ReactorID>,
}

use crate::modules::game_manager::types::*;
impl HostLink {
    fn params(clients: HashMap<PlayerId, ReactorID>) -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self { clients })
            .internal_handler(FunctionHandler::from(i_to_e::<Self, PlayerMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<Self, StateRes>()))
            .external_handler(FunctionHandler::from(Self::handle_from_host))
            .external_handler(FunctionHandler::from(e_to_i::<Self, StateReq>(
                TargetReactor::Reactor,
            )))
    }

    fn handle_from_host(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &HostMsg) {
        let target = match e {
            HostMsg::Data(_, id) => id.clone(),
            HostMsg::Kick(id) => Some(*id),
        };

        if let Some(player_id) = target {
            if let Some(target) = self.clients.get(&player_id) {
                handle.send_internal(e.clone(), TargetReactor::Link(*target));
            }
        } else {
            handle.send_internal(e.clone(), TargetReactor::Links);
        }
    }
}
