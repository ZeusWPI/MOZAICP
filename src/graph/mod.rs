// use crate::messaging::types::ReactorID;
use crate::generic::ReactorID;
use std::mem;
use std::sync::Arc;

use futures::future::Future;
use std::pin::Pin;

mod graph;

pub use self::graph::Graph;

pub static mut GRAPH: Option<Arc<dyn GraphLike>> = None;

pub fn set_default() -> Pin<Box<dyn Future<Output = Option<()>> + Send>> {
    let (graph, fut) = graph::Graph::new();
    set_graph(graph);
    fut
}

pub fn set_graph<T: GraphLike + 'static>(graph: T) {
    let graph = Arc::new(graph);

    unsafe {
        let _ = mem::replace(&mut GRAPH, Some(graph));
    }
}

pub trait GraphLike: Send + Sync {
    fn add_node(&self, id: &ReactorID, name: &str);
    fn add_edge(&self, from: &ReactorID, to: &ReactorID);
    fn remove_node(&self, id: &ReactorID);
    fn remove_edge(&self, from: &ReactorID, to: &ReactorID);
}

pub fn add_node(id: &ReactorID, name: &str) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.add_node(id, name);
        }
    }
}

pub fn add_edge(from: &ReactorID, to: &ReactorID) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.add_edge(from, to);
        }
    }
}

pub fn remove_node(id: &ReactorID) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.remove_node(id);
        }
    }
}

pub fn remove_edge(from: &ReactorID, to: &ReactorID) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.remove_edge(from, to);
        }
    }
}
