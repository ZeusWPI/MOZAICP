use core_capnp::initialize;
use errors::Result;
use messaging::reactor::*;
use messaging::types::*;

use base_capnp::{from_client, host_message, to_client};
use connection_capnp::client_kicked;
use core_capnp::{actors_joined, terminate_stream, close as should_close};

pub struct Aggregator {
    connection_manager: ReactorId,
    host: ReactorId,
    waiting_for: Option<usize>,
}

impl Aggregator {
    pub fn params<C: Ctx>(connection_manager: ReactorId, host: ReactorId) -> CoreParams<Self, C> {
        let me = Self {
            connection_manager,
            host,
            waiting_for: None,
        };
        let mut params = CoreParams::new(me);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(
            actors_joined::Owned,
            CtxHandler::new(Self::handle_actors_joined),
        );

        params.handler(terminate_stream::Owned, CtxHandler::new(Self::handle_actor_left));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()> {
        handle.open_link(HostLink::params(self.host.clone()))?;
        handle.open_link(ConnectionsLink::params(self.connection_manager.clone()))?;

        Ok(())
    }

    fn handle_actor_left<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: terminate_stream::Reader,
    ) -> Result<()> {

        self.waiting_for = self.waiting_for.map(|x| x-1);

        if self.waiting_for == Some(0) {
            let joined = MsgBuffer::<should_close::Owned>::new();
            handle.send_internal(joined)?;
        }

        Ok(())
    }

    fn handle_actors_joined<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: actors_joined::Reader,
    ) -> Result<()> {
        self.waiting_for = Some(0);
        for id in msg.get_ids()?.iter() {
            self.waiting_for = self.waiting_for.map(|x| x+1);
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
        params.external_handler(client_kicked::Owned, CtxHandler::new(client_kicked::e_to_i));

        params.internal_handler(from_client::Owned, CtxHandler::new(from_client::i_to_e));

        params.internal_handler(should_close::Owned, CtxHandler::new(Self::close));

        return params;
    }

    fn close<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: should_close::Reader,
    ) -> Result<()> {

        handle.close_link()?;

        Ok(())
    }
}

struct ConnectionsLink;
impl ConnectionsLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ConnectionsLink);

        params.external_handler(actors_joined::Owned, CtxHandler::new(actors_joined::e_to_i));

        return params;
    }
}

struct ClientLink;
impl ClientLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ClientLink);
        params.internal_handler(host_message::Owned, CtxHandler::new(host_message::i_to_e));
        params.internal_handler(to_client::Owned, CtxHandler::new(to_client::i_to_e));
        params.internal_handler(client_kicked::Owned, CtxHandler::new(client_kicked::i_to_e));

        params.external_handler(terminate_stream::Owned, CtxHandler::new(terminate_stream::e_to_i));
        params.external_handler(from_client::Owned, CtxHandler::new(from_client::e_to_i));

        return params;
    }
}
