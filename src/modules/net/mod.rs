use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::*;

use tokio::io::{AsyncBufReadExt, BufReader, BufWriter, AsyncWriteExt};
use tokio::net::{TcpListener, ToSocketAddrs};
// use tokio::stream::{StreamExt};

use serde_json::Value;

use std::collections::HashMap;

use crate::generic::*;
pub mod types;

mod controller;
pub use controller::ClientController;

pub type PlayerMap = HashMap<types::PlayerId, ReactorID>;

pub struct ConnectionManager {
    pool: ThreadPool,
    target: ReactorID,
    broker: BrokerHandle<String, JSONMessage>,
    player_map: PlayerMap,
}

impl ConnectionManager {
    pub fn new(
        pool: ThreadPool,
        target: ReactorID,
        broker: BrokerHandle<String, JSONMessage>,
        player_map: PlayerMap,
    ) -> CoreParams<Self, String, Message> {
        // Start listening for connections

        //
        let params = CoreParams::new(Self {
            pool,
            target,
            broker,
            player_map,
        });

        params
    }
}

impl ReactorState<String, JSONMessage> for ConnectionManager {
    fn init<'a>(&mut self, _handle: &mut ReactorHandle<'a, String, JSONMessage>) {
        // spawn reactor like with the actually accepting

        // Open ConnectorLink to that reactor lik
    }
}

#[derive(Clone)]
struct Inner {
    send_f: SenderHandle<String, JSONMessage>,
    broker: BrokerHandle<String, JSONMessage>,
    map: PlayerMap,
}

async fn accepting<A: ToSocketAddrs>(
    id: ReactorID,
    conn_man: ReactorID,
    addr: A,
    pool: ThreadPool,
    map: PlayerMap,
    broker: BrokerHandle<String, JSONMessage>,
) -> Option<()> {
    let mut tcp = TcpListener::bind(addr).await.ok()?;
    let mut listener = tcp.incoming();

    let inner = Inner { send_f: broker.get_sender(&conn_man), broker, map };

    loop {
        let mut socket = listener.next().await?.ok()?;
        let inner = inner.clone();

        pool.spawn_ok(
            async move {
                let (rh, wh) = socket.split();
                let mut lines = BufReader::new(rh).lines().fuse();
                let mut writer = BufWriter::new(wh);

                let first_line = lines.next().await?.ok()?;
                let player = serde_json::from_str::<types::Register>(&first_line)
                    .ok()
                    .map(|x| x.player)?;
                let target = *inner.map.get(&player)?;

                let accept = types::Accepted { player, target };
                
                let (tx, rx) = mpsc::unbounded();
                inner.broker.spawn_reactorlike(target, tx);
                
                let client_f = inner.broker.get_sender(&accept.target);

                inner.send_f.send(
                    id, accept
                )?;

                let mut rx = receiver_handle(rx).boxed().fuse();

                loop {
                    select! {
                        v = rx.next() => {
                            match v? {
                                None => break,
                                Some((_, _, v)) => {
                                    writer.write_all(&v.bytes()?).await.ok()?;
                                    writer.flush();
                                }
                            }
                        },
                        v = lines.next() => {
                            let v = v?.ok()?;
                            if let Some(v) = serde_json::from_str::<Value>(&v).ok() {
                                client_f.send(id, v)?;
                            }
                        },
                        complete => break,
                    };
                }

                Some(())
            }
            .map(|_| ()),
        );
    }
}

struct ConnectorLink {}

impl ConnectorLink {
    fn params() -> LinkParams<Self, String, JSONMessage> {
        let mut params = LinkParams::new(Self {});

        params.external_handler(FunctionHandler::from(Self::handle_accept));

        return params;
    }

    fn handle_accept(
        &mut self,
        handle: &mut LinkHandle<String, JSONMessage>,
        accept: &types::Accepted,
    ) {
        handle.send_internal(Typed::from(*accept));
    }
}
