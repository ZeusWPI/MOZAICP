use std::any;
use std::pin::Pin;
use std::collections::VecDeque;

use futures::prelude::*;
use futures::task::{Context, Poll};
use futures::channel::mpsc;

use serde_json::Value;

use super::types::{Accepted, Data, PlayerId, PlayerMsg};
use crate::generic::FromMessage;
use crate::generic::*;

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
    buffer: VecDeque<Value>, // this might be Value
}

impl ClientController {
    pub fn spawn(
        id: ReactorID,
        json_broker: BrokerHandle<String, JSONMessage>,
        t_broker: BrokerHandle<any::TypeId, Message>,
        host: ReactorID,
        connection_manager: ReactorID,
        client_id: PlayerId,
    ) -> Self {
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
            host_rx, client_rx,
            closed: false,
            buffer: VecDeque::new(),
        }
    }

    fn handle_client_disconencted(&mut self) {
        self.client = None;
    }

    fn flush_client(&mut self) {
        if let Some((_, sender)) = &self.client {
            for value in self.buffer.drain(..) {
                if sender.send(self.id, Data { value }).is_none() {
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
            if let Some(Accepted { player: _, target }) = Accepted::from_msg(&key, &mut msg) {
                self.client = Some((*target, self.broker.get_sender(&target)));
            }
        }
    }

    fn handle_client_msg(&mut self, _: String, msg: JSONMessage) {
        let msg = PlayerMsg {
            id: self.client_id,
            value: msg.clone(),
        };

        self.host.1.send(self.id, msg).unwrap();
    }

    fn handle_host_msg(&mut self, key: any::TypeId, mut msg: Message) {
        if key == any::TypeId::of::<Value>() {
            if let Some(value) = Value::from_msg(&key, &mut msg) {
                self.buffer.push_back(value.clone());
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
                println!("ClientController: Target link closed")
            }

            if let Some((_, sender)) = &this.client {
                if sender.close(this.id).is_none() {
                    println!("ClientController: Client link closed")
                }
            }
            return Poll::Ready(());
        }

        Poll::Pending
    }
}
