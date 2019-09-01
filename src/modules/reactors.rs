use messaging::reactor::*;
use messaging::types::*;
use core_capnp::{initialize};
use capnp::traits::{Owned, HasTypeId};

use mozaic_cmd_capnp::{cmd_input, cmd_return, cmd};


use std::io::{self, BufRead};
use std::thread;

use server::runtime::BrokerHandle;

use futures::{Future, Sink, Stream};
use futures::sync::mpsc::channel;

pub trait Parser<C: Ctx>: Send {
    fn parse(
        &mut self,
        input: &str,
        handle: &mut ReactorHandle<C>,
    ) -> Result<Option<String>, capnp::Error>;
}


pub struct CmdReactor<C: Ctx> {
    foreign_id: ReactorId,
    broker: BrokerHandle,
    parser: Box<dyn Parser<C>>,
}

impl<C> CmdReactor<C>
    where C: Ctx {

    pub fn new(broker: BrokerHandle, foreign_id: ReactorId, parser: Box<dyn Parser<C>>) -> Self {

        CmdReactor {
            foreign_id,
            broker,
            parser,
        }
    }

    pub fn params(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        // params.handler(my_capnp::sent_message::Owned, CtxHandler::new(Self::handle_sent));
        params.handler(cmd_input::Owned, CtxHandler::new(Self::handle_command));
        params.handler(cmd_return::Owned, CtxHandler::new(Self::handle_return));

        return params;
    }

    fn handle_initialize(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        handle.open_link(CmdLink.params(handle.id().clone()));

        setup_async_stdin(self.broker.clone(), handle.id().clone());
        Ok(())
    }


    fn handle_return(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: cmd_return::Reader,
    ) -> Result<(), capnp::Error> {
        let msg = r.get_message()?;
        println!("{}", msg);

        Ok(())
    }

    fn handle_command(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: cmd_input::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_input()?;

        match self.parser.parse(msg, handle)? {
            Some(message) => {
                    let mut joined = MsgBuffer::<cmd_return::Owned>::new();
                joined.build(|b| b.set_message(&message));
                handle.send_internal(joined);
            },
            None => {}
        }

        Ok(())
    }
}

struct CmdLink;

impl CmdLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(
            cmd_input::Owned,
            CtxHandler::new(Self::e_handle_cmd),
        );

        return params;
    }

    fn e_handle_cmd<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: cmd_input::Reader,
    ) -> Result<(), capnp::Error> {
        let msg = r.get_input()?;

        let mut joined = MsgBuffer::<cmd_input::Owned>::new();
        joined.build(|b| b.set_input(msg));
        handle.send_internal(joined);

        Ok(())
    }
}


fn stdin() -> impl Stream<Item=String, Error=io::Error> {
    let (mut tx, rx) = channel(1);
    thread::spawn(move || {
        let input = io::stdin();
        for line in input.lock().lines() {
            match tx.send(line).wait() {
                Ok(s) => tx = s,
                Err(_) => break,
            }
        }
    });
    rx.then(|e| e.unwrap())
}


fn setup_async_stdin(mut broker: BrokerHandle, id: ReactorId) {
    tokio::spawn(futures::lazy(move || {
        stdin().for_each(|string| {
            broker.send_message(
                &id,
                &id,
                cmd_input::Owned,
                move |b| {
                    let mut msg: cmd_input::Builder = b.init_as();
                    msg.set_input(&string);
                }
            );
            Ok(())
        })
        .wait()
        .map_err(|_| ())
        .unwrap();
        Ok(())
    }));
}
