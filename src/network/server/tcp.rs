use futures::{Future, Poll, Async, Stream, Sink, AsyncSink};
use futures::sync::mpsc;
use std::io;
use std::net::SocketAddr;
use tokio;
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::Incoming;
use std::collections::HashMap;

use network::lib::protobuf_codec::{ProtobufTransport, MessageStream};
use network::lib::channel::{TransportInstruction, Channel};
use super::router::Router;
use super::routing_table::RoutingTableHandle;
use super::handshake::Handshaker;

use protocol as proto;


pub struct Listener<R>
    where R: Router
{
    incoming: Incoming,
    routing_table: RoutingTableHandle<R>,
}

impl<R> Listener<R>
    where R: Router + Send + 'static
{
    pub fn new(addr: &SocketAddr, routing_table: RoutingTableHandle<R>)
        -> io::Result<Self>
    {
        TcpListener::bind(addr).map(|tcp_listener| {
            Listener {
                routing_table,
                incoming: tcp_listener.incoming(),
            }
        })
    }

    fn handle_connections(&mut self) -> Poll<(), io::Error> {
        // just make sure the user is aware of this
        println!("Olivier is een letterlijke god");

        while let Some(raw_stream) = try_ready!(self.incoming.poll()) {
            let handler = TcpStreamHandler::new(
                self.routing_table.clone(),
                raw_stream
            );
            tokio::spawn(handler);
        }
        return Ok(Async::Ready(()));
    }
}

impl<R> Future for Listener<R>
    where R: Router + Send + 'static
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        match self.handle_connections() {
            Ok(async) => return Ok(async),
            // TODO: gracefully handle this
            Err(e) => panic!("error: {}", e),
        }
    }
}

struct TcpStreamHandler<R>
    where R: Router
{
    stream: MessageStream<TcpStream, proto::Frame>,
    recv: mpsc::Receiver<TransportInstruction>,
    snd: mpsc::Sender<TransportInstruction>,
    connections: HashMap<u32, mpsc::Sender<Vec<u8>>>,
    routing_table: RoutingTableHandle<R>,
}

impl<R> TcpStreamHandler<R>
    where R: Router + Send + 'static
{
    pub fn new(routing_table: RoutingTableHandle<R>, stream: TcpStream) -> Self {
        // TODO: what channel size should we use?
        let (snd, recv) = mpsc::channel(32);
        TcpStreamHandler {
            stream: MessageStream::new(ProtobufTransport::new(stream)),
            connections: HashMap::new(),
            routing_table,
            recv,
            snd,
        }
    }

    pub fn poll_stream(&mut self) -> Poll<(), io::Error> {
        loop {
            let frame = match try_ready!(self.stream.poll()) {
                Some(frame) => frame,
                None => return Ok(Async::Ready(())),
            };

            // take reference to satisfy the borrow checker
            let snd = &self.snd;
            let routing_table = self.routing_table.clone();

            let sender = self.connections.entry(frame.channel_num)
                .or_insert_with(|| {
                    let (sender, channel) = Channel::create(
                        frame.channel_num,
                        snd.clone(),
                    );
                    let handshake = Handshaker::new(
                        routing_table,
                        channel,
                    );
                    tokio::spawn(handshake);
                    return sender;
                });
            // TODO: handle this cleanly
            let res = sender.start_send(frame.data);
            if res != Ok(AsyncSink::Ready) {
                eprintln!("handler is {:?}", res);
            }
        }
    }

    pub fn poll_instructions(&mut self) -> Poll<(), io::Error> {
        loop {
            try_ready!(self.stream.poll_complete());
            let instruction = match self.recv.poll().unwrap() {
                Async::Ready(msg) => msg.unwrap(),
                Async::NotReady => return Ok(Async::NotReady),
            };
            match instruction {
                TransportInstruction::Send { channel_num, data } => {
                    let frame = proto::Frame { channel_num, data };
                    let res = try!(self.stream.start_send(frame));
                    assert!(res.is_ready(), "writing to MessageStream blocked");

                }
                TransportInstruction::Close { channel_num } => {
                    self.connections.remove(&channel_num);
                }
            }
        }
    }

    pub fn poll_(&mut self) -> Poll<(), io::Error> {
        try!(self.poll_instructions());
        return self.poll_stream();
    }
}

impl<R> Future for TcpStreamHandler<R>
    where R: Router + Send + 'static
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        match self.poll_() {
            Ok(async) => Ok(async),
            Err(_) => Ok(Async::Ready(())),
        }
    }
}
