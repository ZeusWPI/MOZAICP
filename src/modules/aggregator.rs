use crate::generic::*;
use crate::modules::types::*;

use std::any;
use std::collections::HashMap;

pub struct Aggregator {
    host_id: ReactorID,
    clients: HashMap<PlayerId, ReactorID>,
    has_been_connected: HashMap<PlayerId, bool>,
}

impl Aggregator {
    pub fn params(
        host_id: ReactorID,
        clients: HashMap<PlayerId, ReactorID>,
    ) -> CoreParams<Self, any::TypeId, Message> {
        let has_been_connected: HashMap<PlayerId, bool> =
            clients.keys().cloned().map(|x| (x, false)).collect();
        CoreParams::new(Aggregator {
            host_id,
            clients,
            has_been_connected,
        })
        .handler(FunctionHandler::from(Self::handle_state_req))
        .handler(FunctionHandler::from(Self::handle_conn))
    }

    fn handle_conn(&mut self, _handle: &mut ReactorHandle<any::TypeId, Message>, conn: &Connect) {
        if let Some(b) = self.has_been_connected.get_mut(&conn.0) {
            *b = true;
        }
    }

    fn handle_state_req(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        req: &StateReq,
    ) {
        handle.send_internal(
            StateRes(req.0, format!("Res:  {:?}", self.has_been_connected)),
            TargetReactor::Link(self.host_id),
        );
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

use super::client_controller::Connect;
struct ClientLink;
impl ClientLink {
    fn params(host_id: ReactorID) -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self)
            .internal_handler(FunctionHandler::from(i_to_e::<Self, HostMsg>()))
            .external_handler(FunctionHandler::from(e_to_i::<Self, PlayerMsg>(
                TargetReactor::Link(host_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<Self, Connect>(
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
