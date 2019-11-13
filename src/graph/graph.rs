use std::sync::{Arc, Mutex};

use messaging::types::ReactorId;

use tokio::sync::mpsc;

use serde::{Deserialize, Serialize};
use ws::Sender;

enum EventWrapper {
    AddNode(u32, String),
    AddEdge(u32, u32),
    RemoveNode(u32),
    RemoveEdge(u32, u32),

    Conn(Sender),
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum Event {
    Init(Init),
    Add(Add),
    Remove(Remove),
}

#[derive(Serialize, Deserialize, Clone)]
struct Init {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "data_type", content = "data")]
enum Add {
    Node(Node),
    Edge(Edge),
}

#[derive(Serialize, Deserialize, Clone)]
struct Remove {
    data_type: String,
    id: u32,
}

#[derive(Serialize, Deserialize, Clone)]
struct Node {
    id: u32,
    label: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Edge {
    id: u32,
    from: u32,
    to: u32,
}

struct GraphState {
    conns: Vec<Sender>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    created_edges: u32,
    rx: mpsc::Receiver<EventWrapper>,
}

fn first_index<T, P>(list: &Vec<T>, mut p: P) -> Option<usize>
where
    P: FnMut(&T) -> bool,
{
    list.iter()
        .enumerate()
        .find_map(|(i, x)| if p(x) { Some(i) } else { None })
}

impl GraphState {
    fn new() -> mpsc::Sender<EventWrapper> {
        let (tx, rx) = mpsc::channel(10);
        let state = GraphState {
            conns: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            created_edges: 0,
            rx,
        };

        tokio::spawn(state);

        return tx;
    }

    fn add_conn(&mut self, conn: Sender) {
        {
            let event = Event::Init(Init {
                edges: self.edges.clone(),
                nodes: self.nodes.clone(),
            });

            if conn
                .send(ws::Message::Text(
                    serde_json::to_string(&event).unwrap_or("Fuck off".to_string()),
                ))
                .is_err()
            {
                error!("Send failed");
            }
        }

        self.conns.push(conn);
    }

    fn add_node(&mut self, id: u32, name: String) {
        let node = Node {
            id: id,
            label: name,
        };

        let event = Event::Add(Add::Node(node.clone()));
        self.nodes.push(node);

        self.emit_event(event);
    }

    fn add_edge(&mut self, from: u32, to: u32) {
        let edge = Edge {
            id: self.get_new_edge_id(),
            from: from,
            to: to,
        };

        let event = Event::Add(Add::Edge(edge.clone()));

        self.edges.push(edge);
        self.emit_event(event);
    }

    fn remove_node(&mut self, id: u32) {
        first_index(&self.nodes, |n| n.id == id).map(|idx| self.nodes.remove(idx));

        let event = Event::Remove(Remove {
            data_type: String::from("Node"),
            id: id,
        });

        self.emit_event(event);
    }

    fn remove_edge(&mut self, from: u32, to: u32) {
        if let Some(id) = first_index(&self.edges, |n| n.from == from && n.to == to)
            .map(|idx| self.edges.remove(idx).id)
        {
            let event = Event::Remove(Remove {
                data_type: String::from("Edge"),
                id: id,
            });
            self.emit_event(event);
        }
    }

    fn emit_event(&mut self, event: Event) {
        for sender in self.conns.iter() {
            if sender
                .send(ws::Message::Text(
                    serde_json::to_string(&event).unwrap_or("Fuck off".to_string()),
                ))
                .is_err()
            {
                error!("Send failed");
            }
        }
    }

    fn get_new_edge_id(&mut self) -> u32 {
        self.created_edges += 1;
        self.created_edges
    }
}

use tokio::prelude::{Async, Future, Poll, Stream};
impl Future for GraphState {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.rx.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(Some(event))) => match event {
                    EventWrapper::Conn(c) => self.add_conn(c),
                    EventWrapper::AddEdge(f, t) => self.add_edge(f, t),
                    EventWrapper::AddNode(f, t) => self.add_node(f, t),
                    EventWrapper::RemoveEdge(f, t) => self.remove_edge(f, t),
                    EventWrapper::RemoveNode(t) => self.remove_node(t),
                },
                _ => return Ok(Async::Ready(())),
            }
        }
    }
}

#[derive(Clone)]
pub struct Graph {
    tx: mpsc::Sender<EventWrapper>,
}

use std::thread;

impl Graph {
    pub fn new() -> Graph {
        let out = Graph {
            tx: GraphState::new(),
        };

        let tx = out.tx.clone();

        thread::spawn(move || {
            ws::listen("127.0.0.1:3012", |out| {
                if let Err(_) = tx.clone().try_send(EventWrapper::Conn(out.clone())) {
                    error!("Couldnt send message to graph");
                }

                move |_| Ok(())
            })
            .unwrap()
        });

        return out;
    }

    pub fn new_boxed() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new()))
    }
}

use super::GraphLike;
impl GraphLike for Graph {
    fn add_node(&self, id: &ReactorId, name: &str) {
        if let Err(_) = self
            .tx
            .clone()
            .try_send(EventWrapper::AddNode(id.as_u32(), String::from(name)))
        {
            error!("Couldnt send message to graph");
        }
    }

    fn add_edge(&self, from: &ReactorId, to: &ReactorId) {
        if let Err(_) = self
            .tx
            .clone()
            .try_send(EventWrapper::AddEdge(from.as_u32(), to.as_u32()))
        {
            error!("Couldn't send message to graph");
        }
    }

    fn remove_node(&self, id: &ReactorId) {
        if let Err(_) = self
            .tx
            .clone()
            .try_send(EventWrapper::RemoveNode(id.as_u32()))
        {
            error!("Couldn't send message to graph");
        }
    }

    fn remove_edge(&self, from: &ReactorId, to: &ReactorId) {
        if let Err(_) = self
            .tx
            .clone()
            .try_send(EventWrapper::RemoveEdge(from.as_u32(), to.as_u32()))
        {
            error!("Couldn't send message to graph");
        }
    }
}
