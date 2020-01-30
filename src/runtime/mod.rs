pub mod runtime;

use std::pin::Pin;
use std::net::SocketAddr;

use futures::prelude::Future;
use futures::task::{Poll, Context as Ctx};
use futures::FutureExt;
// use futures::{Async, Future, Poll};
use tokio::net::TcpListener;

use messaging::types::*;

pub use self::runtime::{Broker, BrokerHandle, Runtime};

use net::server::{ClientHandler, ServerHandler};

pub fn connect_to_server(
    broker: BrokerHandle,
    greeter_id: ReactorId,
    addr: &SocketAddr,
) -> impl Future<Output = ()> {
    tokio::net::TcpStream::connect(addr.clone()).map(|stream| {
        let handler = ClientHandler::new(stream.unwrap(), broker, greeter_id);
        tokio::spawn(handler.then(|_| {
            info!("handler closed");
            futures::future::ready(())
        }));
        ()
    })
}

pub struct TcpServer {
    listener: TcpListener,
    broker: BrokerHandle,
    greeter_id: ReactorId,
}

impl TcpServer {
    pub fn new(
        broker: BrokerHandle,
        greeter_id: ReactorId,
        addr: SocketAddr,
    ) -> impl Future<Output = Self> {
        TcpListener::bind(addr).map(|listener| TcpServer {
            listener: listener.unwrap(),
            broker,
            greeter_id,
        })
    }

    fn handle_incoming(&mut self, ctx: &mut Ctx) -> Poll<()> {
        if !self.broker.reactor_exists(&self.greeter_id) {
            info!("Stopping tcp server for {:?}", self.greeter_id);
            return Poll::Ready(());
        }

        loop {
            if let Ok((stream, _addr)) = ready!(self.listener.poll_accept(ctx)) {
                let handler =
                    ServerHandler::new(stream, self.broker.clone(), self.greeter_id.clone());
                tokio::spawn(handler);
            } else {
                // Can't listen for tcp sockets anymore
                return Poll::Ready(())
            }
        }
    }
}

impl Drop for TcpServer {
    fn drop(&mut self) {
        info!("Tcp server dropped");
    }
}

impl Future for TcpServer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Ctx) -> Poll<Self::Output> {
        self.handle_incoming(ctx)
    }
}
