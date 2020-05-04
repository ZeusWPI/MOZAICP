pub use crate::util::request;
mod builder;
mod manager;
mod runner;

pub use builder::Builder;
pub use manager::Manager;
pub use runner::Runner;

use crate::modules::types::{HostMsg, PlayerId, PlayerMsg};

use serde_json::Value;

pub trait Controller {
    /// on_start get's called when all players are connected for the first time
    fn on_start(&mut self) -> Vec<HostMsg> {
        info!("Starting this game");
        Vec::new()
    }

    /// on_connect is called when a player wants to connect
    /// Returning updates that should be sent to the players
    fn on_connect(&mut self, _player: PlayerId) -> Vec<HostMsg> {
        Vec::new()
    }

    /// on_disconnect is called when a player disconnects
    /// When using a buffering client controller, the message sent to this client are buffered.
    /// Returning updates that should be sent to the players
    fn on_disconnect(&mut self, _player: PlayerId) -> Vec<HostMsg> {
        Vec::new()
    }

    /// on_step is called with at least one turn of a player
    /// Returning updates that should be sent to the players
    fn on_step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg>;

    /// Returns a state of the game
    /// This is used by the game::Manager to store played games
    fn get_state(&mut self) -> Value {
        Value::Null
    }

    /// Check if the game is finished and can be removed
    fn is_done(&mut self) -> Option<Value>;
}

pub type GameBox = Box<dyn Controller + Send>;
