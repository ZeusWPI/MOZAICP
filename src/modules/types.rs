use serde::{Deserialize, Serialize};

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct PlayerData {
    pub value: String,
    pub id: PlayerId,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub enum PlayerMsg {
    Data(PlayerData),
    Timeout(PlayerId),
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct HostData {
    pub value: String,
    pub target: Option<PlayerId>,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub enum HostMsg {
    Data(HostData),
    Kick(PlayerId),
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Data {
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Close {}
