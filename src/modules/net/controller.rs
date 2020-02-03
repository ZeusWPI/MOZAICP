use std::any;
use std::pin::Pin;

use futures::prelude::*;
use futures::task::{Context, Poll};

use serde_json::Value;

use super::types::{Data, PlayerId, PlayerMsg, Accepted};
use crate::generic::*;
use crate::generic::FromMessage;

pub struct ClientController {
    id: ReactorID,
    broker: BrokerHandle<String, JSONMessage>,

    target: (ReactorID, SenderHandle<any::TypeId, Message>),
    conn_man: ReactorID,
    client: Option<(ReactorID, SenderHandle<String, JSONMessage>)>,

    client_id: PlayerId,

    target_rx: Receiver<any::TypeId, Message>,
    client_rx: Receiver<String, JSONMessage>,

    closed: bool,
    buffer: Vec<Value>, // this might be Value
}

impl ClientController {
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
        let msg = PlayerMsg { id: self.client_id, value: msg.clone() };

        self.target.1.send(self.id, msg).unwrap();
    }

    fn handle_target_msg(&mut self, key: any::TypeId, mut msg: Message) {
        if key == any::TypeId::of::<Value>() {
            if let Some(value) = Value::from_msg(&key, &mut msg) {
                self.buffer.push(value.clone());
            }
        }
    }
}

impl Future for ClientController {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);

        // Do target things
        loop {
            match Stream::poll_next(Pin::new(&mut this.client_rx), ctx) {
                Poll::Pending => break,
                Poll::Ready(None) => this.closed = true,
                Poll::Ready(Some(Operation::ExternalMessage(reactor, key, msg))) if reactor == this.conn_man => this.handle_conn_msg(key, msg),
                Poll::Ready(Some(Operation::ExternalMessage(_, key, msg))) => this.handle_client_msg(key, msg),
                Poll::Ready(Some(Operation::CloseLink(reactor))) if reactor == this.conn_man => this.closed = true,
                Poll::Ready(Some(Operation::CloseLink(_))) => this.handle_client_disconencted(),
                _ => { unreachable!() }
            }
        }

        loop {
            match Stream::poll_next(Pin::new(&mut this.target_rx), ctx) {
                Poll::Pending => break,
                Poll::Ready(None) => this.closed = true,
                Poll::Ready(Some(Operation::ExternalMessage(_, key, msg))) => this.handle_target_msg(key, msg),
                Poll::Ready(Some(Operation::CloseLink(_))) => this.closed = true,
                _ => { unreachable!() }
            }
        }

        this.flush_client();

        if this.closed {
            if this.target.1.close(this.id).is_none() {
                println!("ClientController: Target link closed")
            }

            if let Some((_, sender)) = &this.client {
                if sender.close(this.id).is_none() {
                    println!("ClientController: Client link closed")
                }
            }
            return Poll::Ready(())
        }

        Poll::Pending
    }
}
