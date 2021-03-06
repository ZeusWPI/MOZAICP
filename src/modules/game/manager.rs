use super::builder::BoxedBuilder;
use crate::generic::*;
use crate::modules::net::{RegisterGame};
use crate::modules::logger::GameJoin;

use futures::channel::mpsc::{self, UnboundedSender};
use futures::channel::oneshot;
use futures::executor::ThreadPool;
use futures::future::RemoteHandle;
use futures::prelude::*;

use serde_json::Value;

use std::any;

pub mod builder {
    use super::Manager;
    use crate::generic::*;
    use crate::modules::{ClientManager, EndpointBuilder};
    use crate::modules::logger::*;

    use futures::executor::ThreadPool;
    use futures::future::RemoteHandle;

    use std::any;
    use std::marker::PhantomData;

    pub struct ToInsert;
    pub struct Inserted;

    pub struct Builder<Ep, Logger> {
        pd: PhantomData<(Ep, Logger)>,
        broker: BrokerHandle<any::TypeId, Message>,
        eps: Vec<ReactorID>,
        gm_id: ReactorID,
        cm_id: ReactorID,
        logger_id: ReactorID,
    }

    impl<Ep, T> Builder<Ep, T> {
        pub fn add_endpoint<E: EndpointBuilder>(self, ep: E, name: &str) -> Builder<Inserted, T> {
            let Builder {
                pd: _,
                broker,
                mut eps,
                cm_id,
                gm_id,
                logger_id,
            } = self;
            let ep_id = ReactorID::rand();
            let (sender, fut) = ep.build(ep_id, broker.get_sender(&cm_id));

            broker.spawn_reactorlike(ep_id, sender, fut, name);
            eps.push(ep_id);

            Builder {
                pd: PhantomData,
                broker,
                eps,
                gm_id,
                cm_id,
                logger_id,
            }
        }
    }

    impl Builder<ToInsert, ToInsert> {
        pub fn new(pool: ThreadPool) -> (Self, RemoteHandle<()>) {
            let (broker, handle) = BrokerHandle::new(pool);
            (
                Builder {
                    pd: PhantomData,
                    broker,
                    eps: Vec::new(),
                    cm_id: ReactorID::rand(),
                    gm_id: ReactorID::rand(),
                    logger_id: ReactorID::rand(),
                },
                handle,
            )
        }
    }

    use serde_json::Value;
    impl<I> Builder<I, ToInsert> {
        pub fn set_logger<H: LogHandler<Value> + Send + 'static>(self, handler: H, tp: ThreadPool) -> Builder<I, Inserted> {
            let Builder {
                pd: _,
                broker,
                eps,
                cm_id,
                gm_id,
                logger_id,
            } = self;

            let logger = Logger::params(gm_id, handler, tp);
            broker.spawn(logger, Some(logger_id));

            Builder {
                pd: PhantomData,
                broker,
                eps,
                cm_id,
                gm_id,
                logger_id,
            }
        }
    }

    impl Builder<Inserted, ToInsert> {
        pub async fn build<P: AsRef<async_std::path::Path> + Send>(self, p: P, tp: ThreadPool) -> Option<Manager> {
            let log_handler = DefaultLogHandler::new(p).await?;
            Some(self.set_logger(log_handler, tp).build())
        }
    }

    impl Builder<Inserted, Inserted> {
        pub fn build(self) -> Manager {
            let Builder {
                pd: _,
                broker,
                eps,
                cm_id,
                gm_id,
                logger_id,
            } = self;

            let cm_params = ClientManager::new(gm_id, eps);
            broker.spawn(cm_params, Some(cm_id));

            Manager::new(broker, gm_id, cm_id, logger_id)
        }
    }
}

use builder::Builder;

struct GameOpReq(GameOp, oneshot::Sender<GameOpRes>);
impl GameOpReq {
    fn new(inner: GameOp) -> (Self, oneshot::Receiver<GameOpRes>) {
        let (tx, rx) = oneshot::channel();
        (Self(inner, tx), rx)
    }
}

enum GameOp {
    Build(BoxedBuilder),
    Kill(GameID),
    State(GameID),
}

pub enum GameOpRes {
    Built(Option<GameID>),
    State(Option<Result<(Value, Vec<Connect>), Value>>),
    Kill(Option<()>),
}

/// Game manager 'front end'
pub struct Manager {
    op_tx: UnboundedSender<GameOpReq>,
}

impl Manager {
    pub fn builder(pool: ThreadPool) -> (Builder<builder::ToInsert, builder::ToInsert>, RemoteHandle<()>) {
        Builder::new(pool)
    }

    pub fn new(
        broker: BrokerHandle<any::TypeId, Message>,
        self_id: ReactorID,
        cm_id: ReactorID,
        logger_id: ReactorID,
    ) -> Self {
        let op_tx = GameManagerFuture::spawn(broker, self_id, cm_id, logger_id);
        Self { op_tx }
    }

    pub async fn start_game<B: Into<BoxedBuilder>>(&self, builder: B) -> Option<u64> {
        let (req, chan) = GameOpReq::new(GameOp::Build(builder.into()));
        self.op_tx.unbounded_send(req).ok()?;

        if let GameOpRes::Built(x) = chan.await.ok()? {
            x
        } else {
            error!("Got wrong Game Op Response, this should not happen");
            None
        }
    }

    pub async fn get_state(&self, game: u64) -> Option<Result<(Value, Vec<Connect>), Value>> {
        let (req, chan) = GameOpReq::new(GameOp::State(game));
        self.op_tx.unbounded_send(req).ok()?;

        if let GameOpRes::State(x) = chan.await.ok()? {
            x
        } else {
            error!("Got wrong Game Op Response, this should not happen");
            None
        }
    }

    pub async fn kill_game(&self, game: u64) -> Option<()> {
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

use super::request::*;
use std::collections::HashMap;
type GameID = u64;

/// Game manager 'back end'
struct GameManagerFuture {
    broker: BrokerHandle<any::TypeId, Message>,
    games: HashMap<GameID, Result<SenderHandle<any::TypeId, Message>, Value>>,
    requests: HashMap<UUID, oneshot::Sender<GameOpRes>>,

    id: ReactorID,
    cm_id: ReactorID,
    logger_id: ReactorID,

    cm_chan: SenderHandle<any::TypeId, Message>,
    logger_chan: SenderHandle<any::TypeId, Message>,
}

impl GameManagerFuture {
    fn spawn(
        broker: BrokerHandle<any::TypeId, Message>,
        self_id: ReactorID,
        cm_id: ReactorID,
        logger_id: ReactorID,
    ) -> UnboundedSender<GameOpReq> {
        let (op_tx, mut op_rx) = mpsc::unbounded();
        let (ch_tx, ch_rx) = mpsc::unbounded();

        let mut ch_rx = receiver_handle(ch_rx).boxed().fuse();

        let mut this = Self {
            cm_chan: broker.get_sender(&cm_id),
            logger_chan: broker.get_sender(&logger_id),
            broker: broker.clone(),
            games: HashMap::new(),
            requests: HashMap::new(),
            id: self_id,
            cm_id,
            logger_id,
        };

        let fut = async move {
                loop {
                    select! {
                        req = op_rx.next() => {
                            // Handle request
                            if let Some(GameOpReq(req, chan)) = req {
                                let uuid = UUID::new();
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
                                // Handle response
                                if if key == any::TypeId::of::<Res::<(Value, State)>>() {
                                    Res::<(Value, State)>::from_msg(&key, &mut msg).map(|Res(id, (value, state))| {
                                        this.send_msg(*id, GameOpRes::State(Some(Ok((value.clone(), state.res().clone())))))
                                    }).is_none()
                                } else if key == any::TypeId::of::<(u64, Value)>() {
                                    <(u64, Value)>::from_msg(&key, &mut msg).map(|(id, value)| {
                                        this.games.insert(*id, Err(value.clone()))
                                    }).is_none()
                                } else {
                                    Res::<Kill>::from_msg(&key, &mut msg).map(|Res::<Kill>(id, _)| {
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
        if self
            .requests
            .remove(&uuid)
            .and_then(|chan| chan.send(res).ok())
            .is_none()
        {
            error!("Request channel is already used!");
        }
    }

    fn handle_gamebuilder(&mut self, uuid: UUID, builder: BoxedBuilder) {
        let game_uuid = rand::random();
        let (game_id, players) = builder(self.broker.clone(), self.id, self.cm_id, self.logger_id, game_uuid);
        self.cm_chan.send(
            self.id,
            RegisterGame {
                game: game_uuid,
                players,
            },
        );
        info!(%game_id, "Spawning game");
        self.logger_chan.send(self.id, GameJoin(game_id)).unwrap();
        self.games
            .insert(game_uuid, Ok(self.broker.get_sender(&game_id)));

        self.send_msg(uuid, GameOpRes::Built(Some(game_uuid)));
    }

    fn handle_state(&mut self, uuid: UUID, game: GameID) {
        if let Some(ch) = self.games.get(&game) {
            match ch {
                Ok(ch) => if ch.send(self.id, Req(uuid, State::Request)).is_none() {
                    self.send_msg(uuid, GameOpRes::State(None));
                },
                Err(resolved) => {
                    let res = resolved.clone();
                    self.send_msg(uuid, GameOpRes::State(Some(Err(res))));
                }
            }
        } else {
            self.send_msg(uuid, GameOpRes::State(None));
        }
    }

    fn handle_kill(&mut self, uuid: UUID, game: GameID) {
        if let Some(Ok(ch)) = self.games.get(&game) {
            if ch.send(self.id, Req(uuid, Kill)).is_none() {
                self.send_msg(uuid, GameOpRes::Kill(None));
            }
        } else {
            self.send_msg(uuid, GameOpRes::Kill(None));
        }
    }
}
