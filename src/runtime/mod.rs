pub mod runtime;

use std::net::SocketAddr;
use std::io;

use futures::{Future, Poll, Async};
use tokio::net::TcpListener;
use rand::Rng;

use messaging::types::*;
use messaging::reactor::{CoreParams};

pub use self::runtime::{Broker, BrokerHandle, Runtime};

use net::server::{ServerHandler, ClientHandler};

pub fn run_server<F, S>(addr: SocketAddr, initialize_greeter: F)
    where F: Send + 'static + FnOnce(ReactorId) -> CoreParams<S, Runtime>,
          S: Send + 'static
{

    // let stupid = Stupid { greeter_id: greeter_id.clone(), broker: broker.clone()};
    // let mut params: CoreParams<Stupid, Runtime> = stupid.params();

    tokio::run(futures::lazy(move || {

        let mut broker = Broker::new().unwrap();
        let greeter_id: ReactorId = rand::thread_rng().gen();

        let greeter_params = initialize_greeter(broker.get_runtime_id());

        broker.spawn(greeter_id.clone(), greeter_params, &format!("Tcp Server {}", addr.port())).unwrap();

        tokio::spawn(TcpServer::new(broker, greeter_id, &addr));
        return Ok(());
    }));
}

pub fn connect_to_server(broker: BrokerHandle, greeter_id: ReactorId, addr: &SocketAddr) -> impl Future<Item=(), Error=()> {
    tokio::net::TcpStream::connect(addr).map(|stream| {
        let handler = ClientHandler::new(
            stream,
            broker,
            greeter_id,
        );
        tokio::spawn(handler);
    }).map_err(|e| error!("{:?}", e))
}

pub struct TcpServer {
    listener: TcpListener,
    broker: BrokerHandle,
    greeter_id: ReactorId,
}

impl TcpServer {
    pub fn new(broker: BrokerHandle, greeter_id: ReactorId, addr: &SocketAddr)
        -> Self
    {
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
            return Ok(Async::Ready(()))
        }

        loop {
            let (stream, _addr) = try_ready!(self.listener.poll_accept());
            let handler = ServerHandler::new(
                stream,
                self.broker.clone(),
                self.greeter_id.clone()
            );
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
