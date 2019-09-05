

use messaging::reactor::*;
use messaging::types::*;
use errors::{Result, Consumable};
use core_capnp::{initialize};

use core_capnp::{actor_joined, identify};
use network_capnp::{disconnected};
use client_capnp::{client_send, client_message, client_disconnected, client_connected};

use server::runtime::BrokerHandle;

use std::collections::{HashMap, VecDeque};

mod util {

    use std::ops::Deref;

    #[derive(Clone, Debug, Hash, Eq, PartialEq, Copy)]
    pub struct Identifier(u64);

    impl From<u64> for Identifier {
        fn from(src: u64) -> Identifier {
            Identifier(src)
        }
    }

    impl Into<u64> for Identifier {
        fn into(self) -> u64 {
            self.0
        }
    }

    impl Deref for Identifier {
        type Target = u64;

        fn deref(& self) -> &u64 {
            &self.0
        }
    }

    #[derive(Clone, Debug, Hash, Eq, PartialEq, Copy)]
    pub struct PlayerId(u64);

    impl From<u64> for PlayerId {
        fn from(src: u64) -> Self {
            PlayerId(src)
        }
    }

    impl Into<u64> for PlayerId {
        fn into(self) -> u64 {
            self.0
        }
    }

}

use self::util::{Identifier, PlayerId};

struct ClientController {
    connected: bool,        // Is there curerntly a link to the client
    queue: VecDeque<String>, // Queue used when client is not connected
    id: PlayerId,         // With what id is the client connected (used for link seperation)
    key: Identifier,
}

impl ClientController {
    fn new(id: PlayerId, key: Identifier) -> Self {
        Self {
            connected: false,
            queue: VecDeque::new(),
            id: id, key: key
        }
    }

    fn handle_message<C: Ctx>(&mut self, handle: &mut ReactorHandle<C>, state: String) -> Result<()> {
        self.queue.push_back(state);

        self.empty_queue(handle)?;
        Ok(())
    }

    fn empty_queue<C: Ctx>(&mut self, handle: &mut ReactorHandle<C>) -> Result<()> {
        if self.connected {
            while let Some(s) = self.queue.pop_front() {
                let mut joined = MsgBuffer::<client_message::Owned>::new();
                joined.build(|b| {
                    b.set_client_id(self.key.into());
                    b.set_data(&s);
                });
                handle.send_internal(joined).display();
                // TODO: really send this state to the client
            }
        }
        Ok(())
    }

    fn client_connected<C: Ctx> (&mut self, handle: &mut ReactorHandle<C>) -> Result<()> {
        self.connected = true;
        self.empty_queue(handle)?;
        Ok(())
    }

    fn client_disconnected(&mut self) {
        self.connected = false;
    }

    fn player_id(&self) -> PlayerId {
        self.id.clone()
    }
}

// TODO: TIMEOUTS?
pub struct ConnectionManager {
    // broker: BrokerHandle,
    runtime_id: ReactorId,

    client_controllers: HashMap<Identifier, ClientController>,   // handle send to clients

    foreign_id: ReactorId,
}


impl ConnectionManager {

    pub fn params<C: Ctx>(runtime_id: ReactorId, ids: Vec<(Identifier, PlayerId)>, foreign_id: ReactorId) -> CoreParams<Self, C> {
        let client_controllers = ids.iter().cloned()
            .map(|(id, player_id)| (id, ClientController::new(player_id, id)))
            .collect();

        let server_reactor = Self {
            runtime_id, client_controllers, foreign_id
        };

        let mut params = CoreParams::new(server_reactor);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::handle_actor_joined));
        params.handler(client_connected::Owned, CtxHandler::new(Self::handle_connect));
        params.handler(client_disconnected::Owned, CtxHandler::new(Self::handle_disconnect));
        params.handler(client_send::Owned, CtxHandler::new(Self::handle_host_msg));

        return params;
    }

    // TODO change ip endpoint to be identical to cmd reactor (open link to self)
    /// Initialize by opening a link to the ip endpoint
    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        handle.open_link(HostLink::params(self.foreign_id.clone()))?;
        handle.open_link(CreationLink.params(self.runtime_id.clone()))?;
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

    /// Handle client connected by (re)opening it's client controller (it will flush stored messages)
    fn handle_connect<C: Ctx> (
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: client_connected::Reader,
    ) -> Result<()>
    {
        let id = r.get_client_id();
        let id = Identifier::from(id);

        if let Some(client_controller) = self.client_controllers.get_mut(&id) {
            client_controller.client_connected(handle)?;
        }

        Ok(())
    }

    /// Handle client disconnected by setting client's client controller to disconnected
    fn handle_disconnect<C: Ctx> (
        &mut self,
        _: &mut ReactorHandle<C>,
        r: client_disconnected::Reader,
    ) -> Result<()>
    {
        let id = r.get_client_id();
        let id = Identifier::from(id);

        if let Some(client_controller) = self.client_controllers.get_mut(&id) {
            client_controller.client_disconnected();
        }

        Ok(())
    }

    /// Got client send from host, so distribute to all connected clients
    fn handle_host_msg<C: Ctx> (
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: client_send::Reader,
    ) -> Result<()>
    {
        let msg = r.get_data()?;

        for cc in self.client_controllers.values_mut() {
            cc.handle_message(handle, msg.to_string()).display();
        }

        Ok(())
    }
}

struct CreationLink;

impl CreationLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(Self::e_handle_joined),
        );

        return params;
    }

    /// Pass through actor joined events
    fn e_handle_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()> {
        let id = r.get_id()?;

        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id));
        handle.send_internal(joined)?;

        Ok(())
    }
}

struct HostLink;

impl HostLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let me = HostLink;

        let mut params = LinkParams::new(foreign_id, me);

        params.external_handler(client_send::Owned, CtxHandler::new(Self::e_handle_message));

        params.internal_handler(client_message::Owned, CtxHandler::new(Self::i_handle_message));

        return params;
    }

    /// Pass through client send from host
    fn e_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_send::Reader,
    ) -> Result<()> {
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<client_send::Owned>::new();
        joined.build(|b| b.set_data(msg));
        handle.send_internal(joined)?;

        Ok(())
    }

    /// Pass msg sent from client through to the host
    fn i_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_message::Reader,
    ) -> Result<()> {

        let id = r.get_client_id();
        let msg = r.get_data()?;

        let mut joined = MsgBuffer::<client_message::Owned>::new();
        joined.build(|b| {
            b.set_client_id(id);
            b.set_data(msg);
        });
        handle.send_message(joined)?;

        Ok(())
    }
}


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
            client_send::Owned,
            CtxHandler::new(Self::e_handle_message),
        );

        params.external_handler(
            disconnected::Owned,
            CtxHandler::new(Self::e_handle_disconnect),
        );

        params.internal_handler(
            client_message::Owned,
            CtxHandler::new(Self::i_handle_msg),
        );

        return params;
    }

    fn e_handle_identify<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: identify::Reader,
    ) -> Result<()> {
        let id = r.get_key();

        self.key = Some(Identifier::from(id));

        let mut joined = MsgBuffer::<client_connected::Owned>::new();
        joined.build(|b| b.set_client_id(id));
        handle.send_internal(joined)?;

        Ok(())
    }

    fn e_handle_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        msg: client_send::Reader,
    ) -> Result<()> {
        let msg = msg.get_data()?;

        if let Some(key) = self.key {
            let mut inner_msg = MsgBuffer::<client_message::Owned>::new();
            inner_msg.build(|b| {
                b.set_client_id(key.into());
                b.set_data(msg);
            });
            handle.send_internal(inner_msg)?;
        }

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
                b.set_client_id(key.into());
            });
            handle.send_internal(msg)?;
        }

        handle.close_link()?;

        Ok(())
    }

    fn i_handle_msg<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        msg: client_message::Reader,
    ) -> Result<()> {

        if let Some(key) = self.key {
            let send_key: u64 = msg.get_client_id();
            let my_key: u64 = key.into();

            if send_key == my_key {
                let msg = msg.get_data()?;

                let mut inner_msg = MsgBuffer::<client_send::Owned>::new();

                inner_msg.build(|b| {
                    b.set_data(msg);
                });
                handle.send_message(inner_msg)?;
            }
        }

        Ok(())
    }
}
