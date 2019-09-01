use messaging::reactor::*;
use messaging::types::*;
use core_capnp::{initialize};
use mozaic_cmd_capnp::{cmd_input, cmd_return, cmd};


use std::io::{self, BufRead};
use std::thread;

use server::runtime::BrokerHandle;

use futures::{Future, Sink, Stream};
use futures::sync::mpsc::channel;

pub struct Command {
    inner: MsgBuffer<cmd::Reader>,
}

pub type Parser = Box<dyn Fn(&str) -> Result<Command, String> + Send>;


pub struct CmdReactor {
    foreign_id: ReactorId,
    broker: BrokerHandle,
    parser: Parser,
}

impl CmdReactor {
    pub fn new(broker: BrokerHandle, foreign_id: ReactorId, parser: Parser) -> Self {

        CmdReactor {
            foreign_id,
            broker,
            parser,
        }
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        // params.handler(my_capnp::sent_message::Owned, CtxHandler::new(Self::handle_sent));
        params.handler(cmd_input::Owned, CtxHandler::new(Self::handle_command));
        params.handler(cmd_return::Owned, CtxHandler::new(Self::handle_return));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        setup_async_stdin(self.broker.clone(), handle.id().clone());
        Ok(())
    }


    fn handle_return<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: cmd_return::Reader,
    ) -> Result<(), capnp::Error> {
        let msg = r.get_message()?;
        println!("{}", msg);

        Ok(())
    }

    fn handle_command<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: cmd_input::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_input()?;

        match (self.parser)(msg) {
            Ok(command) => {
                let payload = command.inner.get_payload()?;
                handle.send_internal(payload);
            },
            Err(msg) => {
                let mut joined = MsgBuffer::<cmd_return::Owned>::new();
                joined.build(|b| b.set_message(&msg));
                handle.send_internal(joined);
            }
        }

        Ok(())
    }
}

struct CmdLink {

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
