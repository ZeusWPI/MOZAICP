use crate::generic::*;
use crate::modules::net::SpawnPlayer;
use crate::modules::types::*;

use futures::*;

use futures::channel::mpsc;
use futures::io::*;

use async_std::net;
use futures::executor::ThreadPool;

use {
    futures::compat::{Future01CompatExt, Stream01CompatExt},
    futures::{executor::LocalPool, task::LocalSpawnExt},
    futures::{
        io::{copy_buf, BufReader},
        AsyncReadExt, StreamExt,
    },
    tokio_tungstenite::accept_async,
    ws_stream_tungstenite::*,
};

use std::any;
use std::net::SocketAddr;
use std::pin::Pin;

pub struct WSEndpoint {}
impl WSEndpoint {
    pub fn new(
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
    let ws = accept_async(stream).compat().await.ok()?;

    let ws_stream = WsStream::new(ws);

    Some(())
}
