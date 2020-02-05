use serde::{Deserialize, Serialize};

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct PlayerMsg {
    pub value: String,
    pub id: PlayerId,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Data {
    pub value: String,
}
