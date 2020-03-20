use super::net::{PlayerUUIDs, RegisterGame};
use crate::generic::*;
use crate::modules::{GameBuilder, ClientManager, EndpointBuilder};

use futures::channel::mpsc::{self, UnboundedSender};
use futures::channel::oneshot;
use futures::prelude::*;
use futures::executor::ThreadPool;
use futures::future::RemoteHandle;

use std::any;
use std::marker::{PhantomData};

pub mod types {
    use super::UUID;

    #[derive(Clone, Debug)]
    pub struct StateReq(pub UUID);
    #[derive(Clone, Debug)]
    pub struct StateRes(pub UUID, pub String);

    #[derive(Clone, Debug)]
    pub struct KillReq(pub UUID);
    #[derive(Clone, Debug)]
    pub struct KillRes(pub UUID);
}
use types::*;


pub struct ToInsert;
pub struct Inserted;

pub struct GameManagerBuilder<Ep> {
    pd: PhantomData<Ep>,
    broker: BrokerHandle<any::TypeId, Message>,
    eps: Vec<ReactorID>,
    cm_id: ReactorID,
}

impl<Ep> GameManagerBuilder<Ep> {
    pub fn add_endpoint<E: EndpointBuilder>(self, ep: E, name: &str) -> GameManagerBuilder<Inserted> {
        let GameManagerBuilder { pd: _, broker, mut eps, cm_id } = self;
        let ep_id = ReactorID::rand();
        let (sender, fut) = ep.build(ep_id, broker.get_sender(&cm_id));

        broker.spawn_reactorlike(ep_id, sender, fut, name);
        eps.push(ep_id);

        GameManagerBuilder {
            pd: PhantomData,
            broker, eps, cm_id
        }
    }
}

impl GameManagerBuilder<ToInsert> {
    pub fn new(pool: ThreadPool) -> (Self, RemoteHandle<()>) {
        let (broker, handle) = BrokerHandle::new(pool);
        (GameManagerBuilder {
            pd: PhantomData,
            broker,
            eps: Vec::new(),
            cm_id: ReactorID::rand(),
        }, handle)
    }
}

impl GameManagerBuilder<Inserted> {
    pub fn build(self) -> GameManager {
        let gm_id = ReactorID::rand();
        let GameManagerBuilder { pd: _, broker, eps, cm_id } = self;
        let cm_params = ClientManager::new(gm_id, eps);
        broker.spawn(cm_params, Some(cm_id));


        GameManager::new(broker, gm_id, cm_id)
    }
}


struct GameOpReq(GameOp, oneshot::Sender<GameOpRes>);
impl GameOpReq {
    fn new(inner: GameOp) -> (Self, oneshot::Receiver<GameOpRes>) {
        let (tx, rx) = oneshot::channel();
        (Self(inner, tx), rx)
    }
}

enum GameOp {
    Build(GameBuilder),
    Kill(GameID),
    State(GameID),
}

pub enum GameOpRes {
    Built(Option<GameID>),
    State(Option<String>),
    Kill(Option<()>),
}

/// Game manager 'front end'
pub struct GameManager {
    op_tx: UnboundedSender<GameOpReq>,
}

impl GameManager {
    pub fn builder(pool: ThreadPool) -> (GameManagerBuilder<ToInsert>, RemoteHandle<()>) {
        GameManagerBuilder::new(pool)
    }

    pub fn new(
        broker: BrokerHandle<any::TypeId, Message>,
        self_id: ReactorID,
        cm_id: ReactorID,
    ) -> Self {
        let op_tx = GameManagerFuture::spawn(broker, self_id, cm_id);
        Self { op_tx }
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

    pub async fn get_state(&mut self, game: u64) -> Option<String> {
        let (req, chan) = GameOpReq::new(GameOp::State(game));
        self.op_tx.unbounded_send(req).ok()?;

        if let GameOpRes::State(x) = chan.await.ok()? {
            x
        } else {
            error!("Got wrong Game Op Response, this should not happen");
            None
        }
    }

    pub async fn kill_game(&mut self, game: u64) -> Option<()> {
        let (req, chan) = GameOpReq::new(GameOp::Kill(game));
        self.op_tx.unbounded_send(req).ok()?;

        if let GameOpRes::Kill(x) = chan.await.ok()? {
            x
        } else {
            error!("Got wrong Game Op Response, this should not happen");
            None
        }
    }
}

use std::collections::HashMap;
type UUID = u64;
type GameID = u64;

/// Game manager 'back end'
pub struct GameManagerFuture {
    broker: BrokerHandle<any::TypeId, Message>,
    games: HashMap<GameID, SenderHandle<any::TypeId, Message>>,
    requests: HashMap<UUID, oneshot::Sender<GameOpRes>>,

    id: ReactorID,
    cm_id: ReactorID,

    cm_chan: SenderHandle<any::TypeId, Message>,
}

impl GameManagerFuture {
    fn spawn(
        broker: BrokerHandle<any::TypeId, Message>,
        self_id: ReactorID,
        cm_id: ReactorID,
    ) -> UnboundedSender<GameOpReq> {
        let (op_tx, mut op_rx) = mpsc::unbounded();
        let (ch_tx, ch_rx) = mpsc::unbounded();

        let mut ch_rx = receiver_handle(ch_rx).boxed().fuse();


        let mut this = Self {
            cm_chan: broker.get_sender(&cm_id),
            broker: broker.clone(),
            games: HashMap::new(),
            requests: HashMap::new(),
            id: self_id,
            cm_id,
        };

        let fut = async move {
                loop {
                    select! {
                        req = op_rx.next() => {
                            // Handle request
                            if let Some(GameOpReq(req, chan)) = req {
                                let uuid = rand::random();
                                this.requests.insert(uuid, chan);

                                match req {
                                    GameOp::Build(builder) => this.handle_gamebuilder(uuid, builder),
                                    GameOp::State(game) => this.handle_state(uuid, game),
                                    GameOp::Kill(game) => this.handle_kill(uuid, game),
                                }
                            } else {
                                error!("Breaking here here");
                                break;
                            }
                        },
                        res = ch_rx.next() => {
                            if let Some((from, key, mut msg)) = res? {
                                info!(%from, "msg");
                                // Handle response
                                if if key == any::TypeId::of::<StateRes>() {
                                    StateRes::from_msg(&key, &mut msg).map(|StateRes(id, value)| {
                                        this.send_msg(*id, GameOpRes::State(Some(value.clone())))
                                    }).is_none()
                                } else if key == any::TypeId::of::<PlayerUUIDs>() {
                                    PlayerUUIDs::from_msg(&key, &mut msg).map(|x| println!("{:?}", x)).is_none()
                                } else {
                                    KillRes::from_msg(&key, &mut msg).map(|KillRes(id)| {
                                        this.send_msg(*id, GameOpRes::Kill(Some(())))
                                    }).is_none()
                                } {
                                    error!("HELP");
                                }
                            }
                        }
                    }
                }

                Some(())
            }.map(|_| ());

        broker.spawn_reactorlike(self_id, ch_tx, fut, "Game Manager");

        op_tx
    }

    fn send_msg(&mut self, uuid: UUID, res: GameOpRes) {
        info!("Sending msg to gm");
        if self
            .requests
            .remove(&uuid)
            .and_then(|chan| chan.send(res).ok())
            .is_none()
        {
            error!("Request channel is already used!");
        }
    }

    fn handle_gamebuilder(&mut self, uuid: UUID, builder: GameBuilder) {
        let game_uuid = rand::random();
        let (game_id, players) = builder.start(self.broker.clone(), self.id, self.cm_id);
        self.cm_chan.send(
            self.id,
            RegisterGame {
                game: game_uuid,
                players,
            },
        );
        info!(%game_id, "Spawning game");
        self.games
            .insert(game_uuid, self.broker.get_sender(&game_id));

        self.send_msg(uuid, GameOpRes::Built(Some(game_uuid)));
    }

    fn handle_state(&mut self, uuid: UUID, game: GameID) {
        if let Some(ch) = self.games.get(&game) {
            if ch.send(self.id, StateReq(uuid)).is_none() {
                self.send_msg(uuid, GameOpRes::State(None));
            }
        } else {
            self.send_msg(uuid, GameOpRes::State(None));
        }
    }

    fn handle_kill(&mut self, uuid: UUID, game: GameID) {
        if let Some(ch) = self.games.get(&game) {
            if ch.send(self.id, KillReq(uuid)).is_none() {
                self.send_msg(uuid, GameOpRes::Kill(None));
            }
        } else {
            self.send_msg(uuid, GameOpRes::Kill(None));
        }
    }
}
