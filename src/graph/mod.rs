use messaging::types::ReactorId;
use std::sync::{Mutex, Arc};
mod graph;

pub use self::graph::Graph;

pub trait GraphLike: Send + Sync
{
    fn add_node(&self, id: &ReactorId, name: &str);
    fn add_edge(&mut self, from: &ReactorId, to: &ReactorId);
    fn remove_node(&self, id: &ReactorId);
    fn remove_edge(&self, from: &ReactorId, to: &ReactorId);
}

impl GraphLike for () {
    fn add_node(&self, _id: &ReactorId, _name: &str) {}
    fn add_edge(&mut self, _from: &ReactorId, _to: &ReactorId) {}
    fn remove_node(&self, _id: &ReactorId) {}
    fn remove_edge(&self, _from: &ReactorId, _to: &ReactorId) {}
}

pub fn new_empty() -> Arc<Mutex<()>> {
    Arc::new(Mutex::new(()))
}
