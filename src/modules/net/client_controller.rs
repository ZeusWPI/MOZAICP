use crate::generic::*;
use crate::modules::aggregator::InitConnect;
use crate::modules::net::types::*;
use crate::modules::types::*;
use crate::util::request::*;

use std::any;
use std::collections::VecDeque;

struct ClientClosed;

pub struct ClientController {
    client_manager: ReactorID,
    host: ReactorID,
    client_id: PlayerId,
    client: Option<ReactorID>,
    client_name: Option<String>,

    buffer: VecDeque<Data>,
    key: u64,
}

impl ClientController {
    pub fn params(
        client_manager: ReactorID,
        host: ReactorID,
        client_id: PlayerId,
        key: u64,
    ) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Self {
            client_manager,
            host,
            client_id,
            client_name: None,
            client: None,
            buffer: VecDeque::new(),
            key,
        })
        .handler(FunctionHandler::from(Self::handle_host_msg))
        .handler(FunctionHandler::from(Self::handle_client_msg))
        .handler(FunctionHandler::from(Self::handle_conn))
        .handler(FunctionHandler::from(Self::handle_disc))
        .handler(FunctionHandler::from(Self::handle_conn_req))
    }

    fn handle_host_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, m: &HostMsg) {
        match m {
            HostMsg::Data(data, _) => {
                if let Some(target) = self.client {
                    handle.send_internal(data.clone(), TargetReactor::Link(target));
                } else {
                    self.buffer.push_back(data.clone());
                }
            }
            HostMsg::Kick(_) => handle.close(),
        }
    }

    fn handle_client_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, m: &Data) {
        info!(?m, "Got client msg");
        let msg = PlayerMsg {
            id: self.client_id,
            data: Some(m.clone()),
        };

        handle.send_internal(msg, TargetReactor::Link(self.host));
    }

    fn handle_conn_req(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        req: &Req<Connect>,
    ) {
        if let Some(name) = &self.client_name {
            if self.client.is_some() {
                handle.send_internal(
                    req.res(Connect::Connected(self.client_id, name.clone())),
                    TargetReactor::Link(self.host),
                );
            } else {
                handle.send_internal(
                    req.res(Connect::Reconnecting(self.client_id, name.clone())),
                    TargetReactor::Link(self.host),
                );
            }
        } else {
            handle.send_internal(
                req.res(Connect::Waiting(self.client_id, self.key)),
                TargetReactor::Link(self.host),
            );
        }
    }

    fn handle_conn(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, accept: &Accepted) {
        if self.client_name.is_none() {
            handle.send_internal(InitConnect(self.client_id), TargetReactor::Link(self.host));
        }

        info!("Opening link to client");

        let client_link_params = LinkParams::new(())
            .external_handler(FunctionHandler::from(e_to_i::<(), Data>(
                TargetReactor::Reactor,
            )))
            .internal_handler(FunctionHandler::from(i_to_e::<(), Data>()))
            .closer(|_state, handle| {
                handle.send_internal(ClientClosed, TargetReactor::Reactor);
            });

        self.client = Some(accept.client_id);
        self.client_name = Some(accept.name.clone());

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
            .internal_handler(FunctionHandler::from(i_to_e::<(), Res<Connect>>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), InitConnect>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), Req<Connect>>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), HostMsg>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.host, host_link_params, true);

        let cm_link_params =
            LinkParams::new(()).external_handler(FunctionHandler::from(e_to_i::<(), Accepted>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.client_manager, cm_link_params, true);
    }
}
