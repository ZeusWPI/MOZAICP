use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct PlayerMsg {
    pub value: Value,
    pub id: PlayerId,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Data {
    pub value: Value,
}