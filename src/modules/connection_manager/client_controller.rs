

use messaging::reactor::*;
use messaging::types::*;
use errors::{Result, Consumable};
use core_capnp::{initialize};

use core_capnp::{actor_joined};
use client_capnp::{from_client, client_message, to_client, host_message, client_disconnected, inner_to_client};


use std::collections::{VecDeque};

use super::util::{PlayerId};

pub struct CCReactor {
    connected: bool,
    queue: VecDeque<String>,
    id: PlayerId,
    connection_manager: ReactorId,
    host: ReactorId,
}

impl CCReactor {
    pub fn params<C: Ctx>(id: PlayerId, connection_manager: ReactorId, host: ReactorId) -> CoreParams<Self, C> {
        let me = Self {
            connected: false,
            queue: VecDeque::new(),
            id, connection_manager, host,
        };

        let mut params = CoreParams::new(me);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::handle_connect));
        params.handler(client_disconnected::Owned, CtxHandler::new(Self::handle_disconnect));
        params.handler(host_message::Owned, CtxHandler::new(Self::handle_host_msg));

        return params;
    }

    fn empty_queue<C: Ctx>(&mut self, handle: &mut ReactorHandle<C>) -> Result<()> {
        if self.connected {
            while let Some(s) = self.queue.pop_front() {
                let mut joined = MsgBuffer::<inner_to_client::Owned>::new();
                joined.build(|b| {
                    b.set_data(&s);
                });
                handle.send_internal(joined).display();
            }
        }

        Ok(())
    }

    ///
    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        handle.open_link(HostLink::params(self.id.clone(), self.host.clone()))?;

        handle.open_link(ConnectionManagerLink::params(self.id.clone(), self.connection_manager.clone()))?;

        Ok(())
    }

    /// Handle client connected by (re)opening it's client controller (it will flush stored messages)
    fn handle_connect<C: Ctx> (
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()>
    {
        let id: ReactorId = r.get_id()?.into();

        handle.open_link(ClientLink::params(id))?;

        self.connected = true;

        self.empty_queue(handle)?;

        Ok(())
    }

    /// Handle client disconnected, can't send messages for a while
    fn handle_disconnect<C: Ctx> (
        &mut self,
        _: &mut ReactorHandle<C>,
        _: client_disconnected::Reader,
    ) -> Result<()>
    {
        self.connected = false;

        Ok(())
    }

    fn handle_host_msg<C: Ctx> (
        &mut self,
        handle: &mut ReactorHandle<C>,
        msg: host_message::Reader,
    ) -> Result<()>
    {
        let msg = msg.get_data()?;

        self.queue.push_back(msg.to_string());

        self.empty_queue(handle)?;

        Ok(())
    }

}

struct ConnectionManagerLink {
    client_id: PlayerId,
}

impl ConnectionManagerLink {
    fn params<C: Ctx>(client_id: PlayerId, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self { client_id };

        let mut params = LinkParams::new(foreign_id, me);

        params.external_handler(client_disconnected::Owned, CtxHandler::new(Self::e_handle_disconnect));
        params.external_handler(actor_joined::Owned, CtxHandler::new(Self::e_handle_connect));

        return params;
    }

    /// Pass through client send from host
    fn e_handle_disconnect<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: client_disconnected::Reader,
    ) -> Result<()> {

        let joined = MsgBuffer::<client_disconnected::Owned>::new();
        handle.send_internal(joined)?;

        Ok(())
    }

    /// Pass through client send from host
    fn e_handle_connect<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()> {

        let id = r.get_id()?;

        // Only pass message throug if it is ment for my client
        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id));
        handle.send_internal(joined)?;

        Ok(())
    }
}

/// The main link with the host, passing through all messages
struct HostLink {
    client_id: PlayerId,
}

impl HostLink {
    fn params<C: Ctx>(client_id: PlayerId, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self {
            client_id
        };

        let mut params = LinkParams::new(foreign_id, me);

        params.external_handler(host_message::Owned, CtxHandler::new(Self::e_handle_message));
        params.external_handler(to_client::Owned, CtxHandler::new(Self::e_handle_to_client));

        params.internal_handler(client_message::Owned, CtxHandler::new(Self::i_handle_message));

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
        let self_id: u64 = self.client_id.into();

        // Only pass message throug if it is ment for my client
        if id == self_id {
            let msg = r.get_data()?;

            let mut joined = MsgBuffer::<host_message::Owned>::new();
            joined.build(|b| b.set_data(msg));
            handle.send_internal(joined)?;
        }


        Ok(())
    }

    /// Pass msg sent from client through to the host
    fn i_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_message::Reader,
    ) -> Result<()> {

        let id = self.client_id.into();
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


/// Link with the client, passing though disconnects and messages
struct ClientLink {
}

impl ClientLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self { };

        let mut params = LinkParams::new(foreign_id, me);


        params.external_handler(client_message::Owned, CtxHandler::new(Self::e_handle_message));

        params.internal_handler(client_disconnected::Owned, CtxHandler::new(Self::i_handle_disconnect));
        params.internal_handler(inner_to_client::Owned, CtxHandler::new(Self::i_handle_msg));

        return params;
    }

    fn e_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        msg: client_message::Reader,
    ) -> Result<()> {
        let msg = msg.get_data()?;

        let mut inner_msg = MsgBuffer::<client_message::Owned>::new();
        inner_msg.build(|b| {
            b.set_data(msg);
        });
        handle.send_internal(inner_msg)?;

        Ok(())
    }

    fn i_handle_disconnect<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: client_disconnected::Reader,
    ) -> Result<()> {
        handle.close_link()?;

        Ok(())
    }

    fn i_handle_msg<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        msg: inner_to_client::Reader,
    ) -> Result<()> {

        let msg = msg.get_data()?;

        let mut inner_msg = MsgBuffer::<host_message::Owned>::new();

        inner_msg.build(|b| {
            b.set_data(msg);
        });
        handle.send_message(inner_msg)?;

        Ok(())
    }
}
