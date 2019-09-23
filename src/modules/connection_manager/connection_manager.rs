

use messaging::reactor::*;
use messaging::types::*;
use errors::{Result, Consumable};
use core_capnp::{initialize};

use core_capnp::{actor_joined, actors_joined, identify};
use network_capnp::{disconnected};
use connection_capnp::{client_disconnected, client_connected, host_connected};

use server::runtime::BrokerHandle;

use server::TcpServer;
use std::net::SocketAddr;

use std::collections::{HashMap};

use modules::util::{Identifier, PlayerId};
use super::client_controller::CCReactor;

// TODO: TIMEOUTS?
// TODO: Now only distributed messages are supported, add one on one conversation options
/// Main connection manager, creates handles for as many players as asked for
/// Handles disconnects, reconnects etc, host can always send messages to everybody
pub struct ConnectionManager {
    broker: BrokerHandle,

    ids: HashMap<Identifier, PlayerId>,   // handle send to clients

    host: ReactorId,
    addr: SocketAddr,
}

use std::convert::TryInto;
impl ConnectionManager {

    pub fn params<C: Ctx>(broker: BrokerHandle, ids: HashMap<Identifier, PlayerId>, host: ReactorId, addr: SocketAddr) -> CoreParams<Self, C> {
        let server_reactor = Self {
            broker, ids, host, addr
        };

        let mut params = CoreParams::new(server_reactor);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::handle_actor_joined));

        return params;
    }

    /// Initialize by opening a link to the ip endpoint
    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        handle.open_link(CreationLink::params(handle.id().clone()))?;
        handle.open_link(HostLink::params(self.host.clone()))?;

        let mut ids = Vec::new();
        for (key, id) in self.ids.drain() {
            let cc_id = handle.spawn(CCReactor::params(id, handle.id().clone(), self.host.clone()), "Client Controller")?;
            ids.push(cc_id.clone());
            handle.open_link(ClientControllerLink::params(key, cc_id)).display();
        }

        let mut joined = MsgBuffer::<actors_joined::Owned>::new();
        joined.build(move |b| {
            let mut ids_builder = b.reborrow().init_ids(ids.len().try_into().unwrap());

            for (i, id) in ids.iter().enumerate() {
                ids_builder.set(i.try_into().unwrap(), id.bytes());
            }
        });

        handle.send_internal(joined)?;

        tokio::spawn(TcpServer::new(self.broker.clone(), handle.id().clone(), &self.addr));

        Ok(())
    }

    /// Handle actor joined by opening ClientLink to him
    fn handle_actor_joined<C: Ctx> (
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()>
    {
        let id = r.get_id()?;
        handle.open_link(ClientLink::params(id.into()))?;

        Ok(())
    }
}

struct HostLink;

impl HostLink {
    fn params<C: Ctx>(host: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(host, HostLink);

        params.internal_handler(actors_joined::Owned, CtxHandler::new(actors_joined::i_to_e),);

        return params;
    }
}

/// Creation link to pass through actor joined from hopefully self
struct CreationLink;

impl CreationLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, CreationLink);

        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(actor_joined::e_to_i),
        );

        return params;
    }
}

struct ClientControllerLink {
    key: Identifier,
}

impl ClientControllerLink {
    pub fn params<C: Ctx>(key: Identifier, remote_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self {
            key,
        };

        let mut params = LinkParams::new(remote_id, me);

        params.internal_handler(
            client_connected::Owned,
            CtxHandler::new(Self::i_handle_connected),
        );

        params.internal_handler(
            client_disconnected::Owned,
            CtxHandler::new(Self::i_handle_disconnected),
        );

        return params;
    }

    fn i_handle_connected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_connected::Reader,
    ) -> Result<()> {

        let key = r.get_client_key();
        let self_key: u64 = self.key.into();

        if key == self_key {
            let id = r.get_id()?;

            let mut joined = MsgBuffer::<actor_joined::Owned>::new();
            joined.build(|b| b.set_id(id));
            handle.send_message(joined).display();

            let mut host_joined = MsgBuffer::<host_connected::Owned>::new();
            host_joined.build(|b| {
                b.set_client_key(key);
                b.set_id(handle.remote_uuid().bytes());
            });
            handle.send_internal(host_joined).display();
        }

        Ok(())
    }

    fn i_handle_disconnected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_disconnected::Reader,
    ) -> Result<()> {
        let key = r.get_client_key();
        let self_key: u64 = self.key.into();

        if key == self_key {

            let joined = MsgBuffer::<client_disconnected::Owned>::new();
            handle.send_message(joined)?;
        }

        Ok(())
    }
}


/// Link with the client, passing though disconnects and messages
struct ClientLink {
    key: Option<Identifier>,
}

impl ClientLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self {
            key: None,
        };

        let mut params = LinkParams::new(foreign_id, me);

        params.external_handler(
            identify::Owned,
            CtxHandler::new(Self::e_handle_identify),
        );

        params.external_handler(
            disconnected::Owned,
            CtxHandler::new(Self::e_handle_disconnect),
        );

        params.internal_handler(
            host_connected::Owned,
            CtxHandler::new(Self::i_handle_host_connected),
        );

        return params;
    }

    fn e_handle_identify<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: identify::Reader,
    ) -> Result<()> {
        let key = r.get_key();

        let mut joined = MsgBuffer::<client_connected::Owned>::new();
        joined.build(|b| {
            b.set_client_key(key);
            b.set_id(handle.remote_uuid().bytes());
        });
        handle.send_internal(joined)?;

        self.key = Some(Identifier::from(key));

        Ok(())
    }

    fn e_handle_disconnect<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: disconnected::Reader,
    ) -> Result<()> {
        // If not the client is not yet registered, so it doesn't matter
        if let Some(key) = self.key {
            let mut msg = MsgBuffer::<client_disconnected::Owned>::new();

            msg.build(|b| {
                b.set_client_key(key.into());
            });
            handle.send_internal(msg)?;
        }

        handle.close_link()?;

        Ok(())
    }

    fn i_handle_host_connected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: host_connected::Reader,
    ) -> Result<()> {

        if let Some(key) = self.key {
            let self_key: u64 = key.into();
            let key = r.get_client_key();

            if key == self_key {
                let id = r.get_id()?;

                let mut joined = MsgBuffer::<actor_joined::Owned>::new();

                joined.build(|b| {
                    b.set_id(id);
                });
                handle.send_message(joined)?;
            }
        }

        Ok(())
    }
}
