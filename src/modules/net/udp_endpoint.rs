use crate::generic::*;
use crate::modules::net::types::Register;
use crate::modules::net::{EndpointBuilder, SpawnPlayer};
use crate::modules::types::*;

use futures::channel::mpsc;
use futures::stream::StreamExt;
use futures::*;

use async_std::net;

use std::collections::HashMap;
use std::task::Poll;
use std::any;
use std::net::SocketAddr;
use std::pin::Pin;

enum ClientMsg {
    Start(SocketAddr, SenderHandle<any::TypeId, Message>, ReactorID),
    Data(Data, SocketAddr),
    Stop(SocketAddr),
}

pub struct Builder {
    addr: SocketAddr,
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
        UdpEndpoint::build(id, self.addr, cm_chan)
    }
}

pub struct UdpEndpoint;
impl UdpEndpoint {
    /// Spawn reactor_like UdpEndpoint to handle clients connecting to this address
    pub fn new(addr: SocketAddr) -> impl EndpointBuilder {
        Builder { addr }
    }

    fn build(
        id: ReactorID,
        addr: SocketAddr,
        cm_chan: SenderHandle<any::TypeId, Message>,
    ) -> (
        Sender<any::TypeId, Message>,
        Pin<Box<dyn Future<Output = Option<()>> + Send>>,
    ) {
        let (tx, rx) = mpsc::unbounded();
        (tx, accepting(id, addr, rx, cm_chan).boxed())
    }
}

async fn accepting(
    id: ReactorID,
    addr: SocketAddr,
    rx: Receiver<any::TypeId, Message>,
    cm_chan: SenderHandle<any::TypeId, Message>,
) -> Option<()> {
    let mut clients: HashMap<SocketAddr, (SenderHandle<any::TypeId, Message>, ReactorID)> =
        HashMap::new();
    let udp = net::UdpSocket::bind(addr).await.ok()?;
    let (clients_tx, mut clients_rx) = mpsc::unbounded::<ClientMsg>();

    let mut buf = [0; 1024];
    let mut running = true;

    let mut cm_rx = receiver_handle(rx).boxed().fuse();

    while running {
        while let Poll::Ready(Ok((c, addr))) = poll!(udp.recv_from(&mut buf).boxed()) {
            if let Some((client_chan, s_id)) = clients.get(&addr) {
                let value = String::from_utf8(buf[..c].to_vec()).unwrap();

                if client_chan.send(*s_id, Data { value }).is_none() {
                    error!("Something somethings client error");
                };
            } else {
                let player: Register = match serde_json::from_slice(&buf[..c]) {
                    Ok(v) => v,
                    Err(error) => {
                        udp.send_to(error.to_string().as_bytes(), addr).await.ok()?;
                        continue;
                    }
                };

                let c_tx = clients_tx.clone();

                cm_chan.send(
                    id,
                    SpawnPlayer::new(player, move |s_id, cc_chan| {
                        let (tx, rx): (
                            Sender<any::TypeId, Message>,
                            Receiver<any::TypeId, Message>,
                        ) = mpsc::unbounded();

                        (
                            tx,
                            handle_spawn(addr, c_tx, s_id, cc_chan.clone(), rx)
                                .map(move |_| {
                                    cc_chan.close(s_id);
                                    info!("UDP socket closed")
                                })
                                .boxed(),
                            "UDP Client",
                        )
                    }),
                );

                udp.send_to(&buf, addr).await.unwrap();
            }
        }

        while let Poll::Ready(x) = poll!(cm_rx.next()) {
            if let Some(_) = x {
                error!("Udp endpoint got an unexpected message");
            } else {
                running = false;
                break;
            }
        }

        while let Poll::Ready(x) = poll!(clients_rx.next()) {
            match x {
                None => return None,
                Some(ClientMsg::Data(data, addr)) => {
                    if udp.send_to(data.value.as_bytes(), addr).await.is_err() {
                        let _ = clients.remove(&addr);
                    }
                }
                Some(ClientMsg::Start(addr, sender, id)) => {
                    clients.insert(addr, (sender, id));
                }
                Some(ClientMsg::Stop(addr)) => {
                    clients.remove(&addr);
                }
            }
        }
    }

    Some(())
}

async fn handle_spawn(
    addr: SocketAddr,
    udp_chan: mpsc::UnboundedSender<ClientMsg>,
    s_id: ReactorID,
    cc_chan: SenderHandle<any::TypeId, Message>,
    rx: Receiver<any::TypeId, Message>,
) -> Option<()> {
    if udp_chan
        .unbounded_send(ClientMsg::Start(addr.clone(), cc_chan, s_id))
        .is_err()
    {
        error!("Something something UDP endpoint error");
        return None;
    }

    let mut rx = receiver_handle(rx).boxed().fuse();
    loop {
        match rx.next().await {
            Some(Some((_from, key, mut message))) => {
                let data: &Data = Data::from_msg(&key, &mut message)?;
                if udp_chan
                    .unbounded_send(ClientMsg::Data(data.clone(), addr.clone()))
                    .is_err()
                {
                    error!("Something something UDP endpoint error");
                    break;
                }
            }
            _ => break,
        }
    }

    if udp_chan
        .unbounded_send(ClientMsg::Stop(addr.clone()))
        .is_err()
    {
        error!("Something something UDP endpoint error");
        return None;
    }

    Some(())
}
