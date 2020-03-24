use std::sync::{Arc, Mutex};

use crate::generic::ReactorID;

use futures::channel::mpsc;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use ws::Sender;

enum EventWrapper {
    AddNode(u64, String),
    AddEdge(u64, u64),
    RemoveNode(u64),
    RemoveEdge(u64, u64),

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
    id: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Node {
    id: u64,
    label: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Edge {
    id: u64,
    from: u64,
    to: u64,
}

struct GraphState {
    conns: Vec<Sender>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    created_edges: u64,
    // rx: mpsc::UnboundedReceiver<EventWrapper>,
}

fn first_index<T, P>(list: &Vec<T>, mut p: P) -> Option<usize>
where
    P: FnMut(&T) -> bool,
{
    list.iter()
        .enumerate()
        .find_map(|(i, x)| if p(x) { Some(i) } else { None })
}

use futures::future::{Future, FutureExt};
use std::pin::Pin;
impl GraphState {
    fn new() -> (
        mpsc::UnboundedSender<EventWrapper>,
        Pin<Box<dyn Future<Output = Option<()>> + Send>>,
    ) {
        let (tx, mut rx) = mpsc::unbounded();
        let mut this = GraphState {
            conns: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            created_edges: 0,
        };

        let fut = async move {
            loop {
                if let Some(event) = rx.next().await {
                    match event {
                        EventWrapper::Conn(c) => this.add_conn(c),
                        EventWrapper::AddEdge(f, t) => this.add_edge(f, t),
                        EventWrapper::AddNode(f, t) => this.add_node(f, t),
                        EventWrapper::RemoveEdge(f, t) => this.remove_edge(f, t),
                        EventWrapper::RemoveNode(t) => this.remove_node(t),
                    }
                } else {
                    break;
                }
            }
            Some(())
        };

        return (tx, fut.boxed());
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

    fn add_node(&mut self, id: u64, name: String) {
        let node = Node {
            id: id,
            label: name,
        };

        let event = Event::Add(Add::Node(node.clone()));
        self.nodes.push(node);

        self.emit_event(event);
    }

    fn add_edge(&mut self, from: u64, to: u64) {
        let edge = Edge {
            id: self.get_new_edge_id(),
            from: from,
            to: to,
        };

        let event = Event::Add(Add::Edge(edge.clone()));

        self.edges.push(edge);
        self.emit_event(event);
    }

    fn remove_node(&mut self, id: u64) {
        first_index(&self.nodes, |n| n.id == id).map(|idx| self.nodes.remove(idx));

        let event = Event::Remove(Remove {
            data_type: String::from("Node"),
            id: id,
        });

        self.emit_event(event);
    }

    fn remove_edge(&mut self, from: u64, to: u64) {
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

    fn get_new_edge_id(&mut self) -> u64 {
        self.created_edges += 1;
        self.created_edges
    }
}

#[derive(Clone)]
pub struct Graph {
    tx: mpsc::UnboundedSender<EventWrapper>,
}

use std::thread;

impl Graph {
    pub fn new() -> (Graph, Pin<Box<dyn Future<Output = Option<()>> + Send>>) {
        let (tx, fut) = GraphState::new();
        let out = Graph { tx };

        let tx = out.tx.clone();

        thread::spawn(move || {
            ws::listen("127.0.0.1:3012", |out| {
                if let Err(_) = tx.unbounded_send(EventWrapper::Conn(out.clone())) {
                    error!("Couldnt send message to graph");
                }

                move |_| Ok(())
            })
            .unwrap()
        });

        return (out, fut);
    }

    pub fn new_boxed() -> (
        Arc<Mutex<Self>>,
        Pin<Box<dyn Future<Output = Option<()>> + Send>>,
    ) {
        let (me, fut) = Self::new();
        (Arc::new(Mutex::new(me)), fut)
    }
}

use super::GraphLike;
impl GraphLike for Graph {
    fn add_node(&self, id: &ReactorID, name: &str) {
        if let Err(_) = self
            .tx
            .unbounded_send(EventWrapper::AddNode(**id, String::from(name)))
        {
            error!("Couldnt send message to graph");
        }
    }

    fn add_edge(&self, from: &ReactorID, to: &ReactorID) {
        if let Err(_) = self.tx.unbounded_send(EventWrapper::AddEdge(**from, **to)) {
            error!("Couldn't send message to graph");
        }
    }

    fn remove_node(&self, id: &ReactorID) {
        if let Err(_) = self.tx.unbounded_send(EventWrapper::RemoveNode(**id)) {
            error!("Couldn't send message to graph");
        }
    }

    fn remove_edge(&self, from: &ReactorID, to: &ReactorID) {
        if let Err(_) = self
            .tx
            .unbounded_send(EventWrapper::RemoveEdge(**from, **to))
        {
            error!("Couldn't send message to graph");
        }
    }
}
