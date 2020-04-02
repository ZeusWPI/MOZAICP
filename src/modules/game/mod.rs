pub use crate::util::request;
mod builder;
mod manager;
mod runner;

pub use builder::Builder;
pub use manager::Manager;
pub use runner::Runner;

use crate::modules::types::{HostMsg, PlayerMsg};

use serde_json::Value;

pub trait Controller {
    fn start(&mut self) -> Vec<HostMsg>{
        info!("Starting this game");
        Vec::new()
    }
    fn step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg>;
    fn state(&mut self) -> Value {
        Value::Null
    }
    fn is_done(&mut self) -> Option<Value>;
}
pub type GameBox = Box<dyn Controller + Send>;
