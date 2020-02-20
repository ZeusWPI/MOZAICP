use crate::generic::*;
use crate::modules::types::{HostMsg, PlayerMsg};
use crate::modules::GameBuilder;

use futures::channel::mpsc::{self, UnboundedSender};
use futures::channel::oneshot;
use futures::prelude::*;
use futures::task::{Context, Poll};

use std::any;
use std::pin::Pin;
use std::marker::Unpin;

struct GameOpReq(GameOp, oneshot::Sender<GameOpRes>);
impl GameOpReq {
    fn new(inner: GameOp) -> (Self, oneshot::Receiver<GameOpRes>) {
        let (tx, rx) = oneshot::channel();
        (Self(inner, tx), rx)
    }
}

enum GameOp {
    Build(GameBuilder),
}

pub enum GameOpRes {
    Built(Option<u64>),
    State(String),
}

/// Game manager 'front end'
pub struct GameManager {
    op_tx: UnboundedSender<GameOpReq>,
}

impl GameManager {
    pub fn new(broker: BrokerHandle<any::TypeId, Message>) -> (Self, Sender<any::TypeId, Message>) {
        let (ch_tx, op_tx) = GameManagerFuture::new(broker);
        (Self { op_tx }, ch_tx)
    }

    pub async fn start_game(&mut self, builder: GameBuilder) -> Option<u64> {
        let (req, chan) = GameOpReq::new(GameOp::Build(builder));
        self.op_tx.unbounded_send(req).ok()?;

        if let GameOpRes::Built(x) = chan.await.ok()? {
            x
        } else {
            error!("Got wrong Game Op Response, this should not happen");
            None
        }
    }

    pub async fn get_state(&mut self, game: u64) -> String {
        "".to_string()
    }

    pub async fn kill_game(&mut self, game: u64) -> Option<()> {
        Some(())
    }
}

use std::collections::HashMap;
type UUID = u64;

/// Game manager 'back end'
pub struct GameManagerFuture {
    broker: BrokerHandle<any::TypeId, Message>,
    games: HashMap<u64, Sender<any::TypeId, Message>>,
    requests: HashMap<UUID, oneshot::Sender<GameOpRes>>,
}

impl GameManagerFuture {
    fn new(broker: BrokerHandle<any::TypeId, Message>) -> (Sender<any::TypeId, Message>, UnboundedSender<GameOpReq>) {
        let (op_tx, op_rx) = mpsc::unbounded();
        let (ch_tx, ch_rx) = mpsc::unbounded();

        let me = Self {
            broker, games: HashMap::new(), requests: HashMap::new(),
        };

        tokio::spawn(me);
        (ch_tx, op_tx)
    }

    fn handle_gamebuilder(
        &mut self,
        builder: GameBuilder,
    ) {
        
    }
}

impl Future for GameManagerFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cs: &mut Context) -> Poll<Self::Output> {
        let mut this = Pin::into_inner(self);

        Poll::Ready(())
    }
}
