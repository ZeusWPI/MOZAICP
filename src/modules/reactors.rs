use messaging::reactor::*;
use messaging::types::*;
use core_capnp::{initialize};
use my_capnp;

use super::links;

use std::io::{self, BufRead};
use std::thread;

use server::runtime::BrokerHandle;

use futures::{Future, Sink, Stream};
use futures::stream::BoxStream;
use futures::sync::mpsc::channel;

pub struct CmdReactor {
    foreign_id: ReactorId,
}

impl CmdReactor {
    pub fn new(mut broker: BrokerHandle, foreign_id: ReactorId) -> Self {

        let inner = foreign_id.clone();
        tokio::spawn(futures::lazy(move || {
            print!("> ");
            stdin().for_each(|string| {
                broker.send_message(
                    &inner,
                    my_capnp::send_message::Owned,
                    move |b| {
                        let mut msg: my_capnp::send_message::Builder = b.init_as();
                        msg.set_message(&string);
                    }
                );
                print!("> ");
                Ok(())
            })
            .wait()
            .map_err(|_| ())
            .unwrap();
            Ok(())

        }));

        CmdReactor {
            foreign_id,
        }
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(my_capnp::sent_message::Owned, CtxHandler::new(Self::handle_sent));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), capnp::Error>
    {
        let id = handle.id().clone();

        let cmdLink = links::CommandLink { name: "from main reactor to cmd"};
        handle.open_link(cmdLink.params(self.foreign_id.clone(), false));

        Ok(())
    }

    fn handle_sent<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: my_capnp::sent_message::Reader,
    ) -> Result<(), capnp::Error>
    {
        let msg = r.get_message()?;

        println!("CmdReactor got msg {}", msg);

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
