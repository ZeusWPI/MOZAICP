use crate::generic::*;
use crate::modules::types::*;

use std::any;
use std::collections::HashMap;

pub struct Aggregator {
    host_id: ReactorID,
    clients: HashMap<PlayerId, ReactorID>,
}

impl Aggregator {
    pub fn params(host_id: ReactorID, clients: HashMap<PlayerId, ReactorID>,) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Aggregator{ host_id, clients })
    }
}

impl ReactorState<any::TypeId, Message> for Aggregator {
    const NAME: &'static str = "Aggregator";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        for client_id in self.clients.values() {
            handle.open_link(*client_id, ClientLink::params(self.host_id), true);
        }
        handle.open_link(self.host_id, HostLink::params(self.clients.clone()), true);
    }
}

struct ClientLink {
    host_id: ReactorID,
}
impl ClientLink {
    fn params(host_id: ReactorID,) -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self{ host_id })
            .internal_handler(FunctionHandler::from(Self::handle_inc))
            .external_handler(FunctionHandler::from(Self::handle_from_player))
    }

    fn handle_inc(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &HostMsg) {
        handle.send_message(e.clone());
    }

    fn handle_from_player(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &PlayerMsg) {
        handle.send_internal(e.clone(), TargetReactor::Link(self.host_id));
    }
}

struct HostLink {
    clients: HashMap<PlayerId, ReactorID>,
}

impl HostLink {
    fn params(clients: HashMap<PlayerId, ReactorID>,) -> LinkParams<Self, any::TypeId, Message> {
        LinkParams::new(Self{ clients })
            .internal_handler(FunctionHandler::from(Self::handle_inc))
            .external_handler(FunctionHandler::from(Self::handle_from_host))
    }

    fn handle_inc(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &PlayerMsg) {
        handle.send_message(e.clone());
    }

    fn handle_from_host(&mut self, handle: &mut LinkHandle<any::TypeId, Message>, e: &HostMsg) {
        if let Some(player_id) = e.target {
            if let Some(target) = self.clients.get(&player_id) {
                handle.send_internal(e.clone(), TargetReactor::Link(*target));
            }
        } else {
            handle.send_internal(e.clone(), TargetReactor::Links);
        }
    }
}
