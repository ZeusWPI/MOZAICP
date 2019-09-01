use messaging::reactor::*;
use messaging::types::*;

use core_capnp::{initialize};
use log_capnp::{open_log_link, log, inner_log};

use std::fs::File;
use std::io::{Write};

pub struct LogReactor {
    file: File,
    foreign: (ReactorId, String),
}

impl LogReactor {

    pub fn params<C: Ctx, S>(foreign: (ReactorId, S)) -> CoreParams<Self, C>
        where S: Into<String> {
        let file = File::create("log.log").expect("Couldn't create log file");

        let reactor = LogReactor {
            file, foreign: (foreign.0, foreign.1.into())
        };

        let mut params = CoreParams::new(reactor);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(open_log_link::Owned, CtxHandler::new(Self::handle_open));
        params.handler(inner_log::Owned, CtxHandler::new(Self::handle_log));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error> {
        // Open link with foreign
        self.open_link(handle, self.foreign.clone());

        Ok(())
    }

    fn handle_open<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: open_log_link::Reader,
    ) -> Result<(), capnp::Error> {
        // Open link with new person
        let name = r.get_name()?;
        let id = r.get_id()?;

        self.open_link(handle, (ReactorId::from(id), name.to_string()));

        Ok(())
    }

    fn open_link<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        user: (ReactorId, String)
    ) {
        handle.open_link(
            LogLink::params(user)
        );
    }

    fn handle_log<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        r: inner_log::Reader,
    ) -> Result<(), capnp::Error> {
        // Open link with new person
        let user = r.get_name()?;
        let msg = r.get_log()?;

        self.file.write_fmt(format_args!("{}: {}\n", user, msg)).expect("Couldn't write to log file");

        Ok(())
    }
}

pub struct Link;
impl Link {
    pub fn params<C: Ctx>(logger: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(logger, Link);

        params.internal_handler(
            log::Owned,
            CtxHandler::new(Self::i_handle_log)
        );

        return params;
    }

    fn i_handle_log<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: log::Reader,
    ) -> Result<(), capnp::Error> {
        let msg = r.get_log()?;

        let mut joined = MsgBuffer::<log::Owned>::new();
        joined.build(|b| {
            b.set_log(&msg);
        });
        handle.send_message(joined);

        Ok(())
    }
}


struct LogLink {
    name: String,
}

impl LogLink {
    pub fn params<C: Ctx>(foreign: (ReactorId, String)) -> LinkParams<Self, C> {
        let out = LogLink { name: foreign.1 };
        let mut params = LinkParams::new(foreign.0, out);

        params.external_handler(
            log::Owned,
            CtxHandler::new(Self::e_handle_log)
        );

        params.external_handler(
            open_log_link::Owned,
            CtxHandler::new(Self::e_handle_open)
        );

        return params;
    }

    fn e_handle_log<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: log::Reader,
    ) -> Result<(), capnp::Error> {
        let msg = r.get_log()?;

        let mut joined = MsgBuffer::<inner_log::Owned>::new();
        joined.build(|b| {
            b.set_log(&msg);
            b.set_name(&self.name);
        });
        handle.send_internal(joined);

        Ok(())
    }

    fn e_handle_open<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: open_log_link::Reader,
    ) -> Result<(), capnp::Error> {
        let name = r.get_name()?;
        let id = r.get_id()?;

        let mut joined = MsgBuffer::<open_log_link::Owned>::new();
        joined.build(|b| {
            b.set_id(id);
            b.set_name(name);
        });
        handle.send_internal(joined);

        Ok(())
    }
}
