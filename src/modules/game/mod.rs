

pub use crate::util::request;
mod builder;
mod manager;
mod runner;

pub use runner::Runner;
pub use builder::Builder;
pub use manager::Manager;

use crate::modules::types::{HostMsg, PlayerMsg};

pub trait Controller {
    fn step<'a>(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg>;
    fn is_done(&mut self) -> bool;
}
pub type GameBox = Box<dyn Controller + Send>;
