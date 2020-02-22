use crate::generic::*;
use crate::modules::net::SpawnPlayer;
use crate::modules::types::*;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::*;

use tokio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use std::any;
use std::pin::Pin;
use std::net::SocketAddr;

pub struct TcpEndpoint;
impl TcpEndpoint {
    /// Spawn reactor_like TcpEndpoint to handle clients connecting to this address
    pub fn new(
        id: ReactorID,
        addr: SocketAddr,
        cm_chan: SenderHandle<any::TypeId, Message>,
        tp: ThreadPool,
    ) -> (Sender<any::TypeId, Message>, Pin<Box<dyn Future<Output=Option<()>> + Send>>) {
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

    let mut tcp = TcpListener::bind(addr).await.ok()?;
    let mut listener = tcp.incoming().fuse();

    loop {
        select! {
            socket = listener.next() => {
                info!("Got new socket");

                tp.spawn_ok(handle_socket(id, socket?.ok()?, cm_chan.clone(), tp.clone()).map(
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
    stream: TcpStream,
    cm_chan: SenderHandle<any::TypeId, Message>,
    tp: ThreadPool,
) -> Option<()> {
    let (stream, player): (TcpStream, u64) = {
        let mut line = String::new();
        let mut br = BufReader::new(stream);
        br.read_line(&mut line).await.ok()?;

        // Help needed: into_inner loses the underlying data
        Some((br.into_inner(), line.trim().parse().ok()?))
    }?;

    info!(player, "Got new player");

    cm_chan.send(
        id,
        SpawnPlayer::new(player, move |s_id, cc_chan| {
            let (tx, rx): (Sender<any::TypeId, Message>, Receiver<any::TypeId, Message>) =
                mpsc::unbounded();

            // tp.spawn_ok();

            (tx, handle_spawn(stream, s_id, cc_chan, rx).map(|_| info!("Socket closed")).boxed(), "Client")
        }),
    );

    Some(())
}

async fn handle_spawn(
    mut stream: TcpStream,
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
                .map_err(|error| { info!(?error, "Player stream error") })
                .ok()?;
                cc_chan.send(s_id, Typed::from(Data { value })).unwrap();
            },
            complete => {
                trace!("Done");
                break;
            },
        };
    }
    Some(())
}
