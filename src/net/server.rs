use server::runtime::{BrokerHandle};
use messaging::types::{Message, VecSegment, ReactorId};
use errors;
use futures::sync::mpsc;
use tokio::net::TcpStream;

use super::connection_handler::*;

use network_capnp::{connect, connected, publish, disconnected};
use core_capnp::{actor_joined};

pub struct ServerHandler {
    broker: BrokerHandle,
    tx: mpsc::UnboundedSender<Message>,
    pub welcomer_id: ReactorId,
    connecting_id: Option<ReactorId>,
}

impl ServerHandler {
    pub fn new(stream: TcpStream, broker: BrokerHandle, welcomer_id: ReactorId)
        -> ConnectionHandler<Self>
    {
        ConnectionHandler::new(stream, |tx| {
            let mut handler = HandlerCore::new(ServerHandler {
                broker,
                tx,
                welcomer_id,
                connecting_id: None,
            });
            handler.on(publish::Owned, MsgHandler::new(Self::publish_message));
            handler.on(connect::Owned, MsgHandler::new(Self::handle_connect));
            handler.on(disconnected::Owned, MsgHandler::new(Self::handle_disconnected));

            return handler;
        })
    }

    fn handle_connect(&mut self, w: &mut Writer, r: connect::Reader)
        -> Result<(), errors::Error>
    {
        let connecting_id: ReactorId = r.get_id()?.into();
        self.connecting_id = Some(connecting_id.clone());

        info!("Handling connect of {:?}->{:?}", self.welcomer_id, connecting_id);

        self.broker.register(connecting_id.clone(), self.tx.clone());

        self.broker.send_message(&self.welcomer_id, &self.welcomer_id, actor_joined::Owned, |b| {
            let mut joined: actor_joined::Builder = b.init_as();
            joined.set_id(connecting_id.bytes());
        })?;

        w.write(connected::Owned, |b| {
            let mut connected: connected::Builder = b.init_as();
            connected.set_id(self.welcomer_id.bytes());
        });
        return Ok(());
    }

    fn publish_message(&mut self, _w: &mut Writer, r: publish::Reader)
        -> Result<(), errors::Error>
    {
        let vec_segment = VecSegment::from_bytes(r.get_message()?);
        let message = Message::from_segment(vec_segment);
        self.broker.dispatch_message(message)?;
        return Ok(());
    }

    fn handle_disconnected(&mut self, _w: &mut Writer, _: disconnected::Reader)
        -> Result<(), errors::Error>
    {

        if let Some(sender) = &self.connecting_id {
            // TODO: this is the problem, you don't want to send disconnected to the welcomer but to the client controller
            info!("Handling disconnect of {:?}", sender);
            self.broker.unregister(&sender);

            self.broker.send_message(&sender, &self.welcomer_id, disconnected::Owned, |b| {
                let mut joined: disconnected::Builder = b.init_as();
                joined.set_id(sender.bytes());
            })?;
        }

        return Ok(());
    }
}

pub struct ClientHandler {
    broker: BrokerHandle,
    tx: mpsc::UnboundedSender<Message>,
    welcomer_id: ReactorId,
    connecting_id: Option<ReactorId>,
}

impl ClientHandler {
    pub fn new(stream: TcpStream, broker: BrokerHandle, welcomer_id: ReactorId)
        -> ConnectionHandler<Self>
    {
        let mut out = ConnectionHandler::new(stream, |tx| {
            let mut handler = HandlerCore::new(ClientHandler {
                broker,
                tx,
                welcomer_id: welcomer_id.clone(),
                connecting_id: None,
            });
            handler.on(publish::Owned, MsgHandler::new(Self::publish_message));
            handler.on(connected::Owned, MsgHandler::new(Self::handle_connect));
            handler.on(disconnected::Owned, MsgHandler::new(Self::handle_disconnected));

            return handler;
        });

        out.writer().write(connect::Owned, |b| {
            let mut connect: connect::Builder = b.init_as();
            connect.set_id(welcomer_id.bytes());
        });

        out
    }

    fn handle_connect(&mut self, _w: &mut Writer, r: connected::Reader)
        -> Result<(), errors::Error>
    {
        let connecting_id: ReactorId = r.get_id()?.into();
        self.connecting_id = Some(connecting_id.clone());

        self.broker.register(connecting_id.clone(), self.tx.clone());

        self.broker.send_message(&self.welcomer_id, &self.welcomer_id, actor_joined::Owned, |b| {
            let mut joined: actor_joined::Builder = b.init_as();
            joined.set_id(connecting_id.bytes());
        })?;

        return Ok(());
    }

    fn publish_message(&mut self, _w: &mut Writer, r: publish::Reader)
        -> Result<(), errors::Error>
    {
        let vec_segment = VecSegment::from_bytes(r.get_message()?);
        let message = Message::from_segment(vec_segment);
        self.broker.dispatch_message(message)?;
        return Ok(());
    }

    fn handle_disconnected(&mut self, _w: &mut Writer, _: disconnected::Reader)
        -> Result<(), errors::Error>
    {

        if let Some(sender) = &self.connecting_id {
            self.broker.send_message(&sender, &self.welcomer_id, disconnected::Owned, |b| {
                let mut joined: disconnected::Builder = b.init_as();
                joined.set_id(sender.bytes());
            })?;
        }

        return Ok(());
    }
}
