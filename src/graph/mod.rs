use crate::messaging::types::ReactorId;
use std::mem;
use std::sync::Arc;

mod graph;

pub use self::graph::Graph;

pub static mut GRAPH: Option<Arc<dyn GraphLike>> = None;

pub fn set_graph<T: GraphLike + 'static>(graph: T) {
    let graph = Arc::new(graph);

    unsafe {
        mem::replace(&mut GRAPH, Some(graph));
    }
}

pub trait GraphLike: Send + Sync {
    fn add_node(&self, id: &ReactorId, name: &str);
    fn add_edge(&self, from: &ReactorId, to: &ReactorId);
    fn remove_node(&self, id: &ReactorId);
    fn remove_edge(&self, from: &ReactorId, to: &ReactorId);
}

pub fn add_node(id: &ReactorId, name: &str) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.add_node(id, name);
        }
    }
}

pub fn add_edge(from: &ReactorId, to: &ReactorId) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.add_edge(from, to);
        }
    }
}

pub fn remove_node(id: &ReactorId) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.remove_node(id);
        }
    }
}

pub fn remove_edge(from: &ReactorId, to: &ReactorId) {
    unsafe {
        if let Some(g) = &GRAPH {
            g.remove_edge(from, to);
        }
    }
}
