

use messaging::reactor::*;
use messaging::types::*;
use errors;
use core_capnp::{initialize};

use core_capnp::{actor_joined, identify};

use server::runtime::BrokerHandle;

use std::collections::HashMap;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct Identifier(Ed25519Key);

impl <'a> From<&'a [u8]> for Identifier {
    fn from(src: &'a [u8]) -> Identifier {
        assert!(src.len() == 32);

        let mut bytes = [0; 32];
        bytes.copy_from_slice(src);

        return Identifier(bytes);
    }
}

pub struct ServerReactor {
    broker: BrokerHandle,

    client_controllers: HashMap<Identifier, ReactorId>,
    foreign_id: ReactorId,
}

impl ServerReactor {

    pub fn params<C: Ctx>(broker: BrokerHandle, _ids: Vec<Identifier>, foreign_id: ReactorId) -> CoreParams<Self, C> {
        // TODO: Spawn real client controllers
        let client_controllers = HashMap::new();

        let server_reactor = Self {
            broker, client_controllers, foreign_id
        };

        let mut params = CoreParams::new(server_reactor);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::handle_joined));
        params.handler(identify::Owned, CtxHandler::new(Self::handle_identify));


        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        _handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), errors::Error>
    {

        Ok(())
    }

    fn handle_joined<C: Ctx> (
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), errors::Error>
    {
        let _id = r.get_id()?;

        handle.open_link(CreationLink.params(handle.id().clone()))?;

        Ok(())
    }

    fn handle_identify<C: Ctx> (
        &mut self,
        _handle: &mut ReactorHandle<C>,
        r: identify::Reader,
    ) -> Result<(), errors::Error>
    {
        let id = r.get_key()?;
        let id = Identifier::from(id);

        if let Some(_reactor_id) = self.client_controllers.get(&id) {
            // bla bla
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
            CtxHandler::new(Self::e_handle_cmd),
        );

        return params;
    }

    fn e_handle_cmd<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<(), errors::Error> {
        let id = r.get_id()?;

        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id));
        handle.send_internal(joined)?;

        Ok(())
    }
}



struct SimpleLink;

impl SimpleLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            identify::Owned,
            CtxHandler::new(Self::e_handle_identify),
        );

        return params;
    }

    fn e_handle_identify<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: identify::Reader,
    ) -> Result<(), errors::Error> {
        let id = r.get_key()?;

        let mut joined = MsgBuffer::<identify::Owned>::new();
        joined.build(|b| b.set_key(id));
        handle.send_internal(joined)?;


        handle.close_link()?;

        Ok(())
    }
}
