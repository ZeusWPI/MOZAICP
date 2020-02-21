use crate::generic::*;
use crate::modules::net::types::*;
use crate::modules::types::*;

use std::any;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Connect(pub PlayerId);

struct ClientClosed;

pub struct ClientController {
    client_manager: ReactorID,
    host: ReactorID,
    client_id: PlayerId,
    client: Option<ReactorID>,

    buffer: VecDeque<Data>,
}

impl ClientController {
    pub fn params(
        client_manager: ReactorID,
        host: ReactorID,
        client_id: PlayerId,
    ) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Self {
            client_manager,
            host,
            client_id,
            client: None,

            buffer: VecDeque::new(),
        })
        .handler(FunctionHandler::from(Self::handle_host_msg))
        .handler(FunctionHandler::from(Self::handle_client_msg))
        .handler(FunctionHandler::from(Self::handle_conn))
        .handler(FunctionHandler::from(Self::handle_disc))
    }

    fn handle_host_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, m: &HostMsg) {
        match m {
            HostMsg::Data(data, _) => {
                if let Some(target) = self.client {
                    handle.send_internal(data.clone(), TargetReactor::Link(target));
                } else {
                    self.buffer.push_back(data.clone());
                }
            },
            HostMsg::Kick(_) => handle.close(),
        }
    }

    fn handle_client_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, m: &Data) {
        let msg = PlayerMsg {
            id: self.client_id,
            data: Some(m.clone()),
        };

        handle.send_internal(msg, TargetReactor::Reactor);
    }

    fn handle_conn(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, accept: &Accepted) {
        let client_link_params = LinkParams::new(())
            .external_handler(FunctionHandler::from(e_to_i::<(), Data>(TargetReactor::Reactor)))
            .internal_handler(FunctionHandler::from(i_to_e::<(), Data>()))
            .closer(
                |_state, handle| {
                    handle.send_internal(ClientClosed, TargetReactor::Reactor);
                }
            );

        handle.send_internal(Connect(*accept.client_id), TargetReactor::Link(self.host));

        self.client = Some(accept.client_id);
        handle.open_link(accept.client_id, client_link_params, false);

        self.flush_msgs(handle);
    }

    fn handle_disc(&mut self, _handle: &mut ReactorHandle<any::TypeId, Message>, _: &ClientClosed) {
        self.client = None;
    }

    fn flush_msgs(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>) {
        if let Some(target) = self.client {
            for data in self.buffer.drain(..) {
                handle.send_internal(data, TargetReactor::Link(target));
            }
        }
    }
}

impl ReactorState<any::TypeId, Message> for ClientController {
    const NAME: &'static str = "Client Controller";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let host_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), PlayerMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), Connect>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), HostMsg>(TargetReactor::Reactor)));
        handle.open_link(self.host, host_link_params, true);

        let cm_link_params = LinkParams::new(())
            .external_handler(FunctionHandler::from(e_to_i::<(), Accepted>(TargetReactor::Reactor)));
        handle.open_link(self.client_manager, cm_link_params, true);
    }
}
