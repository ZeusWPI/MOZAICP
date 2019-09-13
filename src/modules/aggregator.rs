

use messaging::reactor::*;
use messaging::types::*;
use errors::{Result};
use core_capnp::{initialize};

use core_capnp::{actors_joined};
use client_capnp::{from_client, to_client, host_message};

pub struct Aggregator {
    connection_manager: ReactorId,
    host: ReactorId,
}

impl Aggregator {
    pub fn params<C: Ctx>(connection_manager: ReactorId, host: ReactorId) -> CoreParams<Self, C> {
        let me = Self {
            connection_manager,
            host,
        };
        let mut params = CoreParams::new(me);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(actors_joined::Owned, CtxHandler::new(Self::handle_actors_joined));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {

        handle.open_link(HostLink::params(self.host.clone()))?;
        handle.open_link(ConnectionsLink::params(self.connection_manager.clone()))?;

        Ok(())
    }

    fn handle_actors_joined<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: actors_joined::Reader,
    ) -> Result<()>
    {
        for id in msg.get_ids()?.iter() {
            let id = id.unwrap();
            let client_id = ReactorId::from(id);
            handle.open_link(ClientLink::params(client_id)).unwrap();
        }
        Ok(())
    }
}

struct HostLink;
impl HostLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {

        let mut params = LinkParams::new(remote_id, HostLink);

        params.external_handler(host_message::Owned, CtxHandler::new(host_message::e_to_i));
        params.external_handler(to_client::Owned, CtxHandler::new(to_client::e_to_i));

        params.internal_handler(from_client::Owned, CtxHandler::new(from_client::i_to_e));

        return params;
    }
}


struct ConnectionsLink;
impl ConnectionsLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ConnectionsLink);

        params.external_handler(
            actors_joined::Owned,
            CtxHandler::new(actors_joined::e_to_i),
        );

        return params;
    }
}

struct ClientLink;
impl ClientLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ClientLink);
        params.internal_handler(host_message::Owned, CtxHandler::new(host_message::i_to_e));
        params.internal_handler(to_client::Owned, CtxHandler::new(to_client::i_to_e));

        params.external_handler(from_client::Owned, CtxHandler::new(from_client::e_to_i));

        return params;
    }
}
