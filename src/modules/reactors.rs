use messaging::reactor::*;
use messaging::types::*;
use core_capnp::{initialize};
use my_capnp;

use super::links;

use std::io::{self, BufRead};
use std::thread;

use server::runtime::BrokerHandle;

use futures::{Future, Sink, Stream};
use futures::sync::mpsc::channel;

pub struct CmdReactor {
    foreign_id: ReactorId,
    broker: BrokerHandle,
}

impl CmdReactor {
    pub fn new(broker: BrokerHandle, foreign_id: ReactorId) -> Self {

        CmdReactor {
            foreign_id,
            broker,
        }
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        // params.handler(my_capnp::sent_message::Owned, CtxHandler::new(Self::handle_sent));
        params.handler(my_capnp::send_message::Owned, CtxHandler::new(Self::handle_send));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        let id = handle.id().clone();

        let cmd_link = links::CommandLink { name: "self link"};
        handle.open_link(cmd_link.params(id.clone()));

        let cmd_link = links::CommandLink { name: "cmd to reactor link"};
        handle.open_link(cmd_link.params(self.foreign_id.clone()));

        let mut broker = self.broker.clone();
        tokio::spawn(futures::lazy(move || {
            stdin().for_each(|string| {
                broker.send_message(
                    &id,
                    &id,
                    my_capnp::send_message::Owned,
                    move |b| {
                        let mut msg: my_capnp::send_message::Builder = b.init_as();
                        msg.set_message(&string);
                    }
                );
                Ok(())
            })
            .wait()
            .map_err(|_| ())
            .unwrap();
            Ok(())
        }));

        Ok(())
    }

    fn handle_send<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: my_capnp::send_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_message()?;

        // println!("CmdReactor senD got msg {}", msg);

        let mut joined = MsgBuffer::<my_capnp::sent_message::Owned>::new();
        joined.build(|b| b.set_message(msg));
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
