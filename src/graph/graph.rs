use std::sync::{Arc, Mutex};

use messaging::types::ReactorId;

use serde::{Deserialize, Serialize};
use ws::Sender;

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
    fn new() -> GraphState {
        GraphState {
            conns: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            created_edges: 0,
        }
    }

    fn add_conn(&mut self, conn: Sender) {
        {
            let event = Event::Init(Init {
                edges: self.edges.clone(),
                nodes: self.nodes.clone(),
            });

            if conn.send(ws::Message::Text(serde_json::to_string(&event).unwrap_or("Fuck off".to_string()))).is_err() {
                error!("Send failed");
            }
        }

        self.conns.push(conn);
    }

    fn emit_event(&mut self, event: Event) {
        for sender in self.conns.iter() {
            if sender.send(ws::Message::Text(serde_json::to_string(&event).unwrap_or("Fuck off".to_string()))).is_err() {
                error!("Send failed");
            }
        }
    }

    fn get_new_edge_id(&mut self) -> u32 {
        self.created_edges += 1;
        self.created_edges
    }
}

#[derive(Clone)]
pub struct Graph {
    inner: Arc<Mutex<GraphState>>,
}

use std::thread;

impl Graph {
    pub fn new() -> Graph {
        let gs = Arc::new(Mutex::new(GraphState::new()));
        let out = Graph {
            inner: gs.clone(),
        };

        // TODO setup websockets etc

        thread::spawn(move || {
            ws::listen("127.0.0.1:3012", |out| {
                info!("Got ws connection");
                gs.lock().unwrap().add_conn(out.clone());

                move |_| {
                    Ok(())
                }
            }).unwrap()
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
        let node = Node { id: id.as_u32(), label: String::from(name) };

        let event = Event::Add(Add::Node (node.clone()));

        let mut inner = self.inner.lock().unwrap();
        inner.nodes.push(node);
        inner.emit_event(event);
    }

    fn add_edge(&mut self, from: &ReactorId, to: &ReactorId) {
        let mut inner = self.inner.lock().unwrap();

        let edge = Edge { id: inner.get_new_edge_id(), from: from.as_u32(), to: to.as_u32()};

        let event = Event::Add(Add::Edge (edge.clone()));
        inner.edges.push(edge);
        inner.emit_event(event);
    }

    fn remove_node(&self, id: &ReactorId) {
        let id = id.as_u32();

        let mut inner = self.inner.lock().unwrap();
        first_index(&inner.nodes, |n| n.id == id).map(|idx| inner.nodes.remove(idx));

        let event = Event::Remove(Remove { data_type: String::from("Node"), id: id });
        inner.emit_event(event);
    }

    fn remove_edge(&self, from: &ReactorId, to: &ReactorId) {
        let from = from.as_u32();
        let to = to.as_u32();

        let mut inner = self.inner.lock().unwrap();

        if let Some(id) = first_index(&inner.edges, |n| n.from == from && n.to == to).map(|idx| inner.edges.remove(idx).id) {
            let event = Event::Remove(Remove { data_type: String::from("Edge"), id: id });
            inner.emit_event(event);
        }
    }
}
