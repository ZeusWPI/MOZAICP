use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::*;

use tokio;
use tokio::io::{AsyncBufReadExt, BufReader, BufWriter, AsyncWriteExt};
use tokio::net::{TcpListener, ToSocketAddrs};
// use tokio::stream::{StreamExt};

use serde_json::Value;

use std::collections::HashMap;
use std::net::SocketAddr;

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
    addr: SocketAddr,
}

impl ConnectionManager {
    pub fn params(
        pool: ThreadPool,
        addr: SocketAddr,
        target: ReactorID,
        broker: BrokerHandle<String, JSONMessage>,
        player_map: PlayerMap,
    ) -> CoreParams<Self, String, JSONMessage> {
        //
        let params = CoreParams::new(Self {
            pool,
            target,
            broker,
            player_map,
            addr,
        });

        params
    }
}

impl ReactorState<String, JSONMessage> for ConnectionManager {
    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, String, JSONMessage>) {
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
            ).map(|_| ())
        );

        // Open ConnectorLink to that reactor like

        handle.open_link(a_id, ConnectorLink::params(), true);
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
                let (rh, mut writer) = socket.split();
                let mut lines = BufReader::new(rh).lines().fuse();

                let first_line = lines.next().await?.ok()?;
                let player = serde_json::from_str::<types::Register>(&first_line)
                    .ok()
                    .map(|x| x.player)?;
                println!("Got Player {}", player);

                writer.write_all(b"Hello Player\n").await.unwrap();

                let target = *inner.map.get(&player)?;

                let accept = types::Accepted { player, target };

                let (tx, rx) = mpsc::unbounded();
                inner.broker.spawn_reactorlike(target, tx);

                println!("Loop");


                let client_f = inner.broker.get_sender(&accept.target);
                println!("Loop");

                inner.send_f.send(
                    id, accept
                )?;

                println!("Loop");

                let mut rx = receiver_handle(rx).boxed().fuse();

                loop {
                    println!("Loop");
                    select! {
                        v = rx.next() => {
                            match v? {
                                None => break,
                                Some((_, _, mut v)) => {
                                    let data: &types::Data = v.into_t()?;
                                    writer.write_all(&serde_json::to_vec(&data).ok()?).await.ok()?;
                                    writer.flush();
                                }
                            }
                        },
                        v = lines.next() => {
                            let v = v?.ok()?;
                            if let Some(value) = serde_json::from_str::<Value>(&v).ok() {
                                client_f.send(id, types::PlayerMsg { value, id: player})?;
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
