use super::ClientControllerBuilder;
use crate::generic::*;
use crate::modules::aggregator::InitConnect;
use crate::modules::net::types::*;
use crate::modules::types::*;
use crate::util::request::*;

use std::any;
use std::collections::VecDeque;

struct ClientClosed(PlayerId);

pub struct ClientController {
    client_manager: ReactorID,
    host: ReactorID,
    client_id: PlayerId,
    client: Option<ReactorID>,
    client_name: Option<String>,

    buffer: VecDeque<Data>,
    key: u64,

    try_reconnect: bool,
}

impl ClientController {
    pub fn params(
        client_manager: ReactorID,
        host: ReactorID,
        client_id: PlayerId,
        key: u64,
        try_reconnect: bool,
    ) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Self {
            client_manager,
            host,
            client_id,
            client_name: None,
            client: None,
            buffer: VecDeque::new(),
            key,
            try_reconnect,
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
            handle.send_internal(
                InitConnect(self.client_id, accept.name.clone()),
                TargetReactor::Link(self.host),
            );
        }

        handle.send_internal(
            ClientStateUpdate {
                id: accept.player,
                state: ClientState::Connected,
            },
            TargetReactor::Link(self.host),
        );

        info!("Opening link to client");

        let player_id = accept.player;

        let client_link_params = LinkParams::new(())
            .external_handler(FunctionHandler::from(e_to_i::<(), Data>(
                TargetReactor::Reactor,
            )))
            .internal_handler(FunctionHandler::from(i_to_e::<(), Data>()))
            .closer(move |_state, handle| {
                handle.send_internal(ClientClosed(player_id), TargetReactor::Reactor);
            });

        self.client = Some(accept.client_id);
        self.client_name = Some(accept.name.clone());

        handle.open_link(accept.client_id, client_link_params, false);

        self.flush_msgs(handle);
    }

    fn handle_disc(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        closed: &ClientClosed,
    ) {
        self.client = None;

        handle.send_internal(
            ClientStateUpdate {
                id: closed.0,
                state: ClientState::Disconnected,
            },
            TargetReactor::Link(self.host),
        );

        if !self.try_reconnect {
            handle.close();
        }
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
            .internal_handler(FunctionHandler::from(i_to_e::<(), ClientStateUpdate>()))
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

pub mod spawner {
    use super::{super::SpawnCC, ClientController, ClientControllerBuilder};
    use crate::generic::*;
    use crate::modules::types::PlayerId;
    use std::any;
    use std::hash::Hash;
    use std::sync::Arc;

    pub trait IntoSpawner<K, M>
    where
        K: 'static + Eq + Hash + Send + Unpin,
        M: 'static + Send,
    {
        fn into_spawner(
            self,
            agg_id: ReactorID,
        ) -> Arc<Box<dyn ClientControllerBuilder<K, M> + 'static + Send + Sync>>;
    }

    #[derive(Clone)]
    pub struct SpawnerBuilder {
        try_reconnect: bool,
    }

    impl SpawnerBuilder {
        pub fn new(try_reconnect: bool) -> Self {
            Self { try_reconnect }
        }
    }

    impl IntoSpawner<any::TypeId, Message> for SpawnerBuilder {
        fn into_spawner(self, agg_id: ReactorID) -> SpawnCC {
            Arc::new(Box::new(Spawner {
                agg_id,
                try_reconnect: self.try_reconnect,
            }))
        }
    }

    #[derive(Clone)]
    pub struct Spawner {
        agg_id: ReactorID,
        try_reconnect: bool,
    }

    impl ClientControllerBuilder<any::TypeId, Message> for Spawner {
        fn build<'a>(
            &self,
            spawn: SpawnHandle<'a, any::TypeId, Message>,
            cm_id: ReactorID,
        ) -> (u64, PlayerId, ReactorID, ReactorID) {
            let key = rand::random();
            let player: PlayerId = rand::random();
            let params =
                ClientController::params(cm_id, self.agg_id, player, key, self.try_reconnect);
            let reactor_id = spawn.spawn(params);
            (key, player, reactor_id, self.agg_id)
        }
    }
}
