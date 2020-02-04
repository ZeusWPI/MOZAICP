use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::generic::ReactorID;

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Copy, Key)]
pub struct Register {
    pub player: PlayerId,
}

#[derive(Serialize, Deserialize, Clone, Copy, Key)]
pub struct Accepted {
    pub player: PlayerId,
    pub target: ReactorID,
}

#[derive(Serialize, Deserialize, Key)]
pub struct Data {
    pub value: Value,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Close {}

#[derive(Serialize, Deserialize)]
pub struct PlayerMsg {
    pub value: Value,
    pub id: PlayerId,
}
