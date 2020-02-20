use std::any;
use std::collections::VecDeque;
use std::pin::Pin;

use futures::channel::mpsc;
use futures::prelude::*;
use futures::task::{Context, Poll};

use super::types::Accepted;
use crate::generic::FromMessage;
use crate::generic::*;
use crate::modules::types::{Data, HostMsg, PlayerId, PlayerMsg};

pub struct ClientController {
    id: ReactorID,
    broker: BrokerHandle<String, JSONMessage>,

    host: (ReactorID, SenderHandle<any::TypeId, Message>),
    conn_man: ReactorID,
    client: Option<(ReactorID, SenderHandle<String, JSONMessage>)>,

    client_id: PlayerId,

    host_rx: Receiver<any::TypeId, Message>,
    client_rx: Receiver<String, JSONMessage>,

    closed: bool,
    buffer: VecDeque<Data>, // this might be Value
}

impl ClientController {
    pub fn new(
        id: ReactorID,
        json_broker: BrokerHandle<String, JSONMessage>,
        t_broker: BrokerHandle<any::TypeId, Message>,
        host: ReactorID,
        connection_manager: ReactorID,
        client_id: PlayerId,
    ) -> Self {
        trace!(
            "Creating cc: {:?}, with client {:?} and host {:?}",
            id,
            client_id,
            host
        );
        let host_sender = t_broker.get_sender(&host);

        let (host_tx, host_rx) = mpsc::unbounded();
        let (client_tx, client_rx) = mpsc::unbounded();

        json_broker.spawn_reactorlike(id, client_tx);
        t_broker.spawn_reactorlike(id, host_tx);

        ClientController {
            id,
            broker: json_broker,
            host: (host, host_sender),
            conn_man: connection_manager,
            client: None,
            client_id,
            host_rx,
            client_rx,
            closed: false,
            buffer: VecDeque::new(),
        }
    }

    fn handle_client_disconencted(&mut self) {
        self.client = None;
    }

    fn flush_client(&mut self) {
        if let Some((_, sender)) = &self.client {
            for data in self.buffer.drain(..) {
                if sender.send(self.id, Typed::from(data)).is_none() {
                    // STOP IT
                    break;
                }
            }

            if !self.buffer.is_empty() {
                self.handle_client_disconencted();
            }
        }
    }

    fn handle_conn_msg(&mut self, key: String, mut msg: JSONMessage) {
        if key == <Accepted as Key<String>>::key() {
            if let Some(Accepted {
                player: _,
                client_id,
                contr_id: _,
            }) = Accepted::from_msg(&key, &mut msg)
            {
                self.client = Some((*client_id, self.broker.get_sender(&client_id)));
            }
        }
    }

    fn handle_client_msg(&mut self, _: String, mut msg: JSONMessage) {
        if let Some(data) = msg.into_t::<Data>() {
            let msg = PlayerMsg {
                id: self.client_id,
                data: Some(data.clone()),
            };
            self.host.1.send(self.id, msg).unwrap();
        } else {
            error!("Couldnt parse json msg");
        }
    }

    fn handle_host_msg(&mut self, key: any::TypeId, mut msg: Message) {
        if key == any::TypeId::of::<HostMsg>() {
            if let Some(value) = HostMsg::from_msg(&key, &mut msg) {
                match value {
                    HostMsg::Data(data, _) => self.buffer.push_back(data.clone()),
                    HostMsg::Kick(_) => self.closed = true,
                }
            }
        }
    }
}

impl Future for ClientController {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);

        // Do host things
        loop {
            match Stream::poll_next(Pin::new(&mut this.client_rx), ctx) {
                Poll::Pending => break,
                Poll::Ready(None) => this.closed = true,
                Poll::Ready(Some(Operation::ExternalMessage(reactor, key, msg)))
                    if reactor == this.conn_man =>
                {
                    this.handle_conn_msg(key, msg)
                }
                Poll::Ready(Some(Operation::ExternalMessage(_, key, msg))) => {
                    this.handle_client_msg(key, msg)
                }
                Poll::Ready(Some(Operation::CloseLink(reactor))) if reactor == this.conn_man => {
                    this.closed = true
                }
                Poll::Ready(Some(Operation::CloseLink(_))) => this.handle_client_disconencted(),
                _ => unreachable!(),
            }
        }

        loop {
            match Stream::poll_next(Pin::new(&mut this.host_rx), ctx) {
                Poll::Pending => break,
                Poll::Ready(None) => this.closed = true,
                Poll::Ready(Some(Operation::ExternalMessage(_, key, msg))) => {
                    this.handle_host_msg(key, msg)
                }
                Poll::Ready(Some(Operation::CloseLink(_))) => this.closed = true,
                _ => unreachable!(),
            }
        }

        this.flush_client();

        if this.closed {
            if this.host.1.close(this.id).is_none() {
                trace!("Target link already closed")
            }

            if this
                .broker
                .get_sender(&this.conn_man)
                .close(this.id)
                .is_none()
            {
                trace!("Connection manager already closed");
            }

            if let Some((_, sender)) = &this.client {
                if sender.close(this.id).is_none() {
                    trace!("Client link closed")
                }
            }

            return Poll::Ready(());
        }

        Poll::Pending
    }
}