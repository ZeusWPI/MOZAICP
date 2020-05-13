use crate::generic::*;
use crate::modules::net::{EndpointBuilder, SpawnPlayer};
use crate::modules::net::types::Register;
use crate::modules::types::*;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::*;

use futures::io::*;

use async_std::net;

use std::any;
use std::net::SocketAddr;
use std::pin::Pin;

pub struct Builder {
    addr: SocketAddr,
    tp: ThreadPool,
}

impl EndpointBuilder for Builder {
    fn build(
        self,
        id: ReactorID,
        cm_chan: SenderHandle<any::TypeId, Message>,
    ) -> (
        Sender<any::TypeId, Message>,
        Pin<Box<dyn Future<Output = Option<()>> + Send>>,
    ) {
        TcpEndpoint::build(id, self.addr, cm_chan, self.tp)
    }
}

pub struct TcpEndpoint;
impl TcpEndpoint {
    /// Spawn reactor_like TcpEndpoint to handle clients connecting to this address
    pub fn new(addr: SocketAddr, tp: ThreadPool) -> impl EndpointBuilder {
        Builder { addr, tp }
    }

    fn build(
        id: ReactorID,
        addr: SocketAddr,
        cm_chan: SenderHandle<any::TypeId, Message>,
        tp: ThreadPool,
    ) -> (
        Sender<any::TypeId, Message>,
        Pin<Box<dyn Future<Output = Option<()>> + Send>>,
    ) {
        let (tx, rx) = mpsc::unbounded();
        (tx, accepting(id, addr, rx, cm_chan, tp).boxed())
    }
}

async fn accepting(
    id: ReactorID,
    addr: SocketAddr,
    rx: Receiver<any::TypeId, Message>,
    cm_chan: SenderHandle<any::TypeId, Message>,
    tp: ThreadPool,
) -> Option<()> {
    let mut rx = receiver_handle(rx).boxed().fuse();

    let tcp = net::TcpListener::bind(addr).await.ok()?;
    let mut listener = tcp.incoming().fuse();

    loop {
        select! {
            socket = listener.next() => {
                info!("Got new socket");

                tp.spawn_ok(handle_socket(id, socket?.ok()?, cm_chan.clone()).map(
                    |_| ()
                ));
            },
            item = rx.next() => {
                if item.is_none() {
                    info!("TCP closing");
                    break;
                } else {
                    error!("Tcp endpoint got an unexpected message");
                }
            }
        }
    }

    Some(())
}

async fn handle_socket(
    id: ReactorID,
    stream: net::TcpStream,
    cm_chan: SenderHandle<any::TypeId, Message>,
) -> Option<()> {
    let (stream, player): (net::TcpStream, Register) = {
        let mut line = String::new();
        let mut br = BufReader::new(stream);
        br.read_line(&mut line).await.ok()?;

        let mut stream = br.into_inner();

        let reg: Register = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(error) => {
                stream.write_all(error.to_string().as_bytes()).await.ok()?;
                return None;
            }
        };

        info!("Got line {:?}", reg);

        // TODO: Help needed: into_inner loses the underlying data
        Some((stream, reg))
    }?;

    info!(?player, "Got new player");

    cm_chan.send(
        id,
        SpawnPlayer::new(player, move |s_id, cc_chan| {
            let (tx, rx): (Sender<any::TypeId, Message>, Receiver<any::TypeId, Message>) =
                mpsc::unbounded();

            (
                tx,
                handle_spawn(stream, s_id, cc_chan.clone(), rx)
                    .map(move |_| {
                        cc_chan.close(s_id);
                        info!("Socket closed")
                    })
                    .boxed(),
                "Client",
            )
        }),
    );

    Some(())
}

async fn handle_spawn(
    stream: net::TcpStream,
    s_id: ReactorID,
    cc_chan: SenderHandle<any::TypeId, Message>,
    rx: Receiver<any::TypeId, Message>,
) -> Option<()> {
    let mut rx = receiver_handle(rx).boxed().fuse();
    let (rh, mut writer) = stream.split();
    let mut lines = BufReader::new(rh).lines().fuse();

    loop {
        select! {
            v = rx.next() => {
                match v? {
                    None => { trace!("Closing"); break; },
                    Some((_from, key, mut message)) => {
                        let data: &Data = Data::from_msg(&key, &mut message)?;
                        writer.write_all(&data.value).await
                            .map_err(|error| { info!(?error, "Write to player failed")})
                            .ok()?;
                        writer.write_all(b"\n").await.ok()?;
                        writer.flush().await.ok()?;
                    }
                }
            },
            v = lines.next() => {
                let value = match v? {
                    Ok(v) => v.into_bytes(),
                    Err(err) => {
                        info!(?err, "Player stream error");
                        break;
                    }
                };
                if cc_chan.send(s_id, Data { value }).is_none() {
                    error!("Something something client error");
                };
            },
            complete => {
                trace!("Done");
                break;
            },
        };
    }

    Some(())
}
