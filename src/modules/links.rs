
use messaging::reactor::*;
use messaging::types::*;
use my_capnp;

/// Extenal Handle is to handle msgs coming from SOMEWHERE else
pub struct CommandLink {
    pub name: &'static str,
}

impl CommandLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            my_capnp::send_message::Owned,
            CtxHandler::new(Self::e_handle_send_msg),
        );

        params.external_handler(
            my_capnp::sent_message::Owned,
            CtxHandler::new(Self::e_handle_sent_msg),
        );

        params.internal_handler(
            my_capnp::sent_message::Owned,
            CtxHandler::new(Self::i_handle_sent_msg),
        );

        return params;
    }

    fn e_handle_send_msg<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: my_capnp::send_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_message()?;
        print!("> ");

        // println!("{}: handling external msg {}", self.name, msg);

        let mut joined = MsgBuffer::<my_capnp::send_message::Owned>::new();
        joined.build(|b| b.set_message(msg));
        handle.send_internal(joined);

        Ok(())
    }

    fn e_handle_sent_msg<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: my_capnp::sent_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_message()?;

        println!("{}: {}", self.name, msg);

        Ok(())
    }

    fn i_handle_sent_msg<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: my_capnp::sent_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_message()?;

        // println!("{}: handling internal sent msg {}", self.name, msg);

        let mut joined = MsgBuffer::<my_capnp::sent_message::Owned>::new();
        joined.build(|b| b.set_message(msg));
        handle.send_message(joined);

        Ok(())
    }
}
