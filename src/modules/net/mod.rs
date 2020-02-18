use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::*;

use tracing::instrument;
use tracing_futures::Instrument;

use tokio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, ToSocketAddrs};

use std::collections::HashMap;
use std::net::SocketAddr;

use super::types::*;
use crate::generic::*;
mod types;
pub use types::Register;
use types::*;

mod controller;
pub use controller::ClientController;

// TODO: add better tracing

pub type PlayerMap = HashMap<PlayerId, ReactorID>;

pub type CheckPlayer = Box<dyn Fn(PlayerId) -> Option<ReactorID>>;

pub struct ConnectionManager {
    pool: ThreadPool,
    broker: BrokerHandle<String, JSONMessage>,
    player_map: PlayerMap,
    addr: SocketAddr,
    connected: usize,
}

impl ConnectionManager {
    pub fn params(
        pool: ThreadPool,
        addr: SocketAddr,
        broker: BrokerHandle<String, JSONMessage>,
        player_map: PlayerMap,
    ) -> CoreParams<Self, String, JSONMessage> {
        let connected = player_map.len();
        CoreParams::new(Self {
            pool,
            broker,
            player_map,
            addr,
            connected,
        })
        .handler(FunctionHandler::from(Self::handle_accept))
        .handler(FunctionHandler::from(Self::cc_closed))
    }

    fn handle_accept(&mut self, handle: &mut ReactorHandle<String, JSONMessage>, acc: &Accepted) {
        handle.send_internal(Typed::from(acc.clone()), TargetReactor::Link(acc.contr_id));
    }

    fn cc_closed(&mut self, handle: &mut ReactorHandle<String, JSONMessage>, _: &Close) {
        self.connected -= 1;
        if self.connected == 0 {
            handle.close();
        }
    }
}

impl ReactorState<String, JSONMessage> for ConnectionManager {
    const NAME: &'static str = "Connection Manager";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, String, JSONMessage>) {
        // Open links to cc's
        for cc_id in self.player_map.values() {
            handle.open_link(*cc_id, CCLink::params(), false);
        }

        // spawn reactor like with the actually accepting
        let a_id = ReactorID::rand();
        tokio::spawn(
            accepting(
                a_id,
                *handle.id(),
                self.addr.clone(),
                self.pool.clone(),
                self.player_map.clone(),
                self.broker.clone(),
            )
            .map(|_| ()),
        );

        // Open ConnectorLink to that reactor like
        handle.open_link(a_id, ConnectorLink::params(), true);
    }
}

struct ConnectorLink {}
impl ConnectorLink {
    fn params() -> LinkParams<Self, String, JSONMessage> {
        LinkParams::new(Self {}).external_handler(FunctionHandler::from(Self::handle_accept))
    }

    fn handle_accept(&mut self, handle: &mut LinkHandle<String, JSONMessage>, accept: &Accepted) {
        handle.send_internal(Typed::from(accept.clone()), TargetReactor::Reactor);
    }
}

struct CCLink {}
impl CCLink {
    fn params() -> LinkParams<Self, String, JSONMessage> {
        LinkParams::new(Self {})
            .internal_handler(FunctionHandler::from(Self::handle_accept))
            .closer(|_, handle| {
                handle.send_internal(Typed::from(Close {}), TargetReactor::Reactor);
            })
    }

    fn handle_accept(&mut self, handle: &mut LinkHandle<String, JSONMessage>, accept: &Accepted) {
        handle.send_message(Typed::from(accept.clone()));
    }
}

#[derive(Clone)]
struct Inner {
    send_f: SenderHandle<String, JSONMessage>,
    broker: BrokerHandle<String, JSONMessage>,
    map: PlayerMap,
}

#[instrument(skip(pool, map, broker))]
async fn accepting<A: ToSocketAddrs + std::fmt::Debug>(
    id: ReactorID,
    conn_man: ReactorID,
    addr: A,
    pool: ThreadPool,
    map: PlayerMap,
    broker: BrokerHandle<String, JSONMessage>,
) -> Option<()> {
    let mut tcp = TcpListener::bind(addr).await.ok()?;
    let mut listener = tcp.incoming();

    let inner = Inner {
        send_f: broker.get_sender(&conn_man),
        broker,
        map,
    };

    loop {
        let mut socket = listener.next().await?.ok()?;
        let inner = inner.clone();

        info!("New connecting client");
        pool.spawn_ok(
            async move {
                let (rh, mut writer) = socket.split();
                let mut lines = BufReader::new(rh).lines().fuse();

                let first_line = lines.next().await?.ok()?;
                let player = serde_json::from_str::<Register>(&first_line)
                    .map_err(|error| info!(?error, "Player couldn't register"))
                    .ok()
                    .map(|x| x.player)?;

                info!(player, "Trying");

                let client_controller = *inner.map.get(&player).or_else(|| {
                    info!(player, "Player not found");
                    None
                })?;
                let cc_f = inner.broker.get_sender(&client_controller);

                let client = ReactorID::rand();

                let accept = Accepted {
                    player,
                    client_id: client,
                    contr_id: client_controller,
                };

                info!(?accept, "Registering player");

                let (tx, rx) = mpsc::unbounded();
                inner.broker.spawn_reactorlike(client, tx);

                inner.send_f.send(id, Typed::from(accept))?;

                let mut rx = receiver_handle(rx).boxed().fuse();
                loop {
                    select! {
                        v = rx.next() => {
                            match v? {
                                None => { trace!("Closing"); break; },
                                Some((_, _, mut v)) => {
                                    let data: &Data = v.into_t()?;
                                    writer.write_all(data.value.as_bytes()).await
                                        .map_err(|error| { info!(?error, "Write to player failed")})
                                        .ok()?;
                                    writer.write_all(b"\n").await.ok()?;
                                    writer.flush();
                                }
                            }
                        },
                        v = lines.next() => {
                            let value = v?
                                .map_err(|error| { info!(?error, "Player stream error")})
                                .ok()?;
                            cc_f.send(id, Typed::from(Data { value })).unwrap();
                        },
                        complete => {
                            trace!("Done");
                            break;
                        },
                    };
                }

                Some(())
            }
            .map(|_| ())
            .instrument(trace_span!("TCP client")),
        );
    }
}
