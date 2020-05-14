use crate::generic::*;
use crate::modules::net::types::Register;
use crate::modules::net::{EndpointBuilder, SpawnPlayer};
use crate::modules::types::*;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::stream::StreamExt;
use futures::*;

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
        WsEndpoint::build(id, self.addr, cm_chan, self.tp)
    }
}

pub struct WsEndpoint;
impl WsEndpoint {
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

use futures::stream::{SplitStream};

async fn get_next_msg<S>(stream: &mut SplitStream<S>) -> Option<Vec<u8>>
    where S: Stream<Item=std::result::Result<tungstenite::Message, tungstenite::Error>> {
        loop {
            let msg = stream.next().await?.ok()?;
            match msg {
                tungstenite::Message::Text(t) => return Some(t.into_bytes()),
                tungstenite::Message::Binary(b) => return Some(b),
                _ => {}
            }
        }
}

async fn handle_socket(
    id: ReactorID,
    stream: net::TcpStream,
    cm_chan: SenderHandle<any::TypeId, Message>,
) -> Option<()> {

    let ws_stream = async_tungstenite::accept_async(stream)
        .await
        .ok()?;

    let (mut write, mut read) = ws_stream.split();

    let first_line = get_next_msg(&mut read).await?;

    let reg: Register = match serde_json::from_slice(&first_line) {
        Ok(v) => v,
        Err(error) => {
            write.send(tungstenite::Message::text(error.to_string())).await.ok()?;
            return None;
        }
    };

    let ws_stream = write.reunite(read).ok()?;

    cm_chan.send(
        id,
        SpawnPlayer::new(reg, move |s_id, cc_chan| {
            let (tx, rx): (Sender<any::TypeId, Message>, Receiver<any::TypeId, Message>) =
                mpsc::unbounded();

            (
                tx,
                handle_spawn(ws_stream, s_id, cc_chan.clone(), rx)
                    .map(move |_| {
                        cc_chan.close(s_id);
                        info!("WS Socket closed")
                    })
                    .boxed(),
                "WS Client",
            )
        }),
    );

    Some(())
}

async fn handle_spawn(
    stream: async_tungstenite::WebSocketStream<net::TcpStream>,
    s_id: ReactorID,
    cc_chan: SenderHandle<any::TypeId, Message>,
    rx: Receiver<any::TypeId, Message>,
) -> Option<()> {
    let mut rx = receiver_handle(rx).boxed().fuse();
    let (mut write, mut read) = stream.split();

    loop {
        select! {
            v = rx.next() => {
                match v? {
                    None => { trace!("Closing"); break; },
                    Some((_from, key, mut message)) => {
                        let data: &Data = Data::from_msg(&key, &mut message)?;
                        let msg = tungstenite::Message::Binary(data.value.clone());
                        write.send(msg).await.ok()?;
                    }
                }
            },
            v = get_next_msg(&mut read).fuse() => {
                let value = v?;
                if cc_chan.send(s_id, Data { value }).is_none() {
                    error!("Something something ws client error");
                };
            },
            complete => {
                trace!("WS client done");
                break;
            },
        };
    }

    Some(())
}
