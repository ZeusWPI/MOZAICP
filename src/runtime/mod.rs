pub mod runtime;

use std::io;
use std::net::SocketAddr;

use futures::{Async, Future, Poll};
use tokio::net::TcpListener;

use messaging::types::*;

pub use self::runtime::{Broker, BrokerHandle, Runtime};

use net::server::{ClientHandler, ServerHandler};

pub fn connect_to_server(
    broker: BrokerHandle,
    greeter_id: ReactorId,
    addr: &SocketAddr,
) -> impl Future<Item = (), Error = ()> {
    tokio::net::TcpStream::connect(addr)
        .map(|stream| {
            let handler = ClientHandler::new(stream, broker, greeter_id);
            tokio::spawn(handler.then(|_| {
                info!("handler closed");
                Ok(())
            }));
        })
        .map_err(|e| error!("{:?}", e))
}

pub struct TcpServer {
    listener: TcpListener,
    broker: BrokerHandle,
    greeter_id: ReactorId,
}

impl TcpServer {
    pub fn new(broker: BrokerHandle, greeter_id: ReactorId, addr: &SocketAddr) -> Self {
        let listener = TcpListener::bind(addr).unwrap();

        return TcpServer {
            listener,
            broker,
            greeter_id,
        };
    }

    fn handle_incoming(&mut self) -> Poll<(), io::Error> {
        if !self.broker.reactor_exists(&self.greeter_id) {
            info!("Stopping tcp server for {:?}", self.greeter_id);
            return Ok(Async::Ready(()));
        }

        loop {
            let (stream, _addr) = try_ready!(self.listener.poll_accept());
            let handler = ServerHandler::new(stream, self.broker.clone(), self.greeter_id.clone());
            tokio::spawn(handler);
        }
    }
}

impl Future for TcpServer {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        Ok(self.handle_incoming().expect("failed to accept connection"))
    }
}
