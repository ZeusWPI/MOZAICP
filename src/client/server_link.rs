use std::sync::{Arc, Mutex};

use client::runtime::{Runtime, RuntimeState, spawn_reactor};
use messaging::reactor::CoreParams;
use messaging::types::{Message, VecSegment, ReactorId};

use tokio::net::TcpStream;
use rand::Rng;

use net::connection_handler::*;

use network_capnp::{connect, connected, publish, disconnected};

pub struct ClientParams {
    pub runtime_id: ReactorId,
    pub greeter_id: ReactorId,
}

// TODO: get rid of this type parameter
pub struct LinkHandler<S> {
    runtime: Arc<Mutex<RuntimeState>>,

    // TODO: UGH clean up this mess wtf
    client_id: ReactorId,
    spawner: Option<Box<dyn Fn(ClientParams) -> CoreParams<S, Runtime> + Send>>,
}

/// ? Why does this LinkHandler can just set the runtime server to itself?
/// ? Because all Runtimes 'need' to have one tcp stream, otherweise it would just be a reactor.
/// ?
/// ? This LinkHandler starts the communication with a server. (See h.writer().write ...)
/// ? I suppose it gets a messages back connected::Owned, with the servers own 'greeter_id'.
/// ? When this happens it can set the runtime server to itself, because the communication has been started.
/// ? This is what happens with that |tx| closure, which is called in the handle_connected.
impl<S> LinkHandler<S>
    where S: Send + 'static
{
    pub fn new<F>(
        stream: TcpStream,
        runtime: Arc<Mutex<RuntimeState>>,
        spawner: F
    ) -> ConnectionHandler<Self>
        where F: 'static + Send + Fn(ClientParams) -> CoreParams<S, Runtime>
    {
        println!("This is never called");
        let client_id: ReactorId = rand::thread_rng().gen();

        let mut h = ConnectionHandler::new(stream, |tx| {
            // TODO: this way of bootstrapping is not ideal
            runtime.lock().unwrap().set_server_link(tx);

            let mut handler = HandlerCore::new(LinkHandler {
                runtime,
                client_id: client_id.clone(),
                spawner: Some(Box::new(spawner)),
            });
            handler.on(publish::Owned, MsgHandler::new(Self::publish_message));
            handler.on(connected::Owned, MsgHandler::new(Self::handle_connected));
            handler.on(disconnected::Owned, MsgHandler::new(Self::handle_disconnected));

            return handler;
        });

        h.writer().write(connect::Owned, |b| {
            let mut b: connect::Builder = b.init_as();
            b.set_id(client_id.bytes());
        });

        return h;
    }

    fn handle_connected(&mut self, _w: &mut Writer, r: connected::Reader)
        -> Result<(), capnp::Error>
    {
        let spawner = match self.spawner.take() {
            None => return Ok(()),
            Some(spawner) => spawner,
        };

        let greeter_id: ReactorId = r.get_id()?.into();

        println!("connected to {:?}!", greeter_id);

        let r = spawner(ClientParams {
            runtime_id: self.runtime.lock().unwrap().runtime_id().clone(),
            greeter_id,
        });
        spawn_reactor(&self.runtime, self.client_id.clone(), r);
        return Ok(());
    }


    fn publish_message(&mut self, _w: &mut Writer, r: publish::Reader)
        -> Result<(), capnp::Error>
    {
        let vec_segment = VecSegment::from_bytes(r.get_message()?);
        let message = Message::from_segment(vec_segment);
        self.runtime.lock().unwrap().dispatch_message(message);
        return Ok(());
    }

    fn handle_disconnected(&mut self, _w: &mut Writer, _:disconnected::Reader)
        -> Result<(), capnp::Error>
    {
        println!("DISCONNEcTING");
        Ok(())
    }
}
