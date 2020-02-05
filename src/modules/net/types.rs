use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::generic::ReactorID;

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Register {
    pub player: PlayerId,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Accepted {
    pub player: PlayerId,
    pub client_id: ReactorID,
    pub contr_id: ReactorID,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Data {
    pub value: Value,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Close {}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct PlayerMsg {
    pub value: Value,
    pub id: PlayerId,
}
