

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
        params.external_handler(to_client::Owned, CtxHandler::new(Self::e_handle_to_client));

        params.internal_handler(from_client::Owned, CtxHandler::new(Self::i_handle_message));

        return params;
    }

    /// Pass through client send from host
    fn e_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: host_message::Reader,
    ) -> Result<()> {
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<host_message::Owned>::new();
        joined.build(|b| b.set_data(msg));
        handle.send_internal(joined)?;

        Ok(())
    }

    /// Pass through client send from host
    fn e_handle_to_client<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: to_client::Reader,
    ) -> Result<()> {
        let id = r.get_client_id();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<to_client::Owned>::new();
        joined.build(|b| {
            b.set_data(msg);
            b.set_client_id(id);
        });
        handle.send_internal(joined)?;

        Ok(())
    }

    /// Pass msg sent from client through to the host
    fn i_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: from_client::Reader,
    ) -> Result<()> {

        let id = r.get_client_id().into();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<from_client::Owned>::new();
        joined.build(|b| {
            b.set_client_id(id);
            b.set_data(msg);
        });
        handle.send_message(joined)?;

        Ok(())
    }
}


struct ConnectionsLink;
impl ConnectionsLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ConnectionsLink);

        params.external_handler(
            actors_joined::Owned,
            CtxHandler::new(Self::e_handle_actors_joined),
        );

        return params;
    }

    fn e_handle_actors_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actors_joined::Reader,
    ) -> Result<()> {
        let ids = r.get_ids()?;

        let mut joined = MsgBuffer::<actors_joined::Owned>::new();
        joined.build(|b| b.set_ids(ids).expect("Fuck off here"));
        handle.send_internal(joined)?;

        Ok(())
    }
}

struct ClientLink;
impl ClientLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, ClientLink);
        params.internal_handler(host_message::Owned, CtxHandler::new(Self::i_handle_message));
        params.internal_handler(to_client::Owned, CtxHandler::new(Self::i_handle_to_client));

        params.external_handler(from_client::Owned, CtxHandler::new(Self::e_handle_message));

        return params;
    }

    fn i_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: host_message::Reader,
    ) -> Result<()> {
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<host_message::Owned>::new();
        joined.build(|b| b.set_data(msg));
        handle.send_message(joined)?;

        Ok(())
    }

    fn i_handle_to_client<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: to_client::Reader,
    ) -> Result<()> {
        let id = r.get_client_id();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<to_client::Owned>::new();
        joined.build(|b| {
            b.set_data(msg);
            b.set_client_id(id);
        });
        handle.send_message(joined)?;

        Ok(())
    }

    fn e_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: from_client::Reader,
    ) -> Result<()> {

        let id = r.get_client_id();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<from_client::Owned>::new();
        joined.build(|b| {
            b.set_client_id(id);
            b.set_data(msg);
        });
        handle.send_internal(joined)?;

        Ok(())
    }
}
