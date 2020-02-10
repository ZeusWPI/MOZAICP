use serde::{Deserialize, Serialize};

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct PlayerMsg {
    pub id: PlayerId, 
    pub data: Option<Data>,
}

// #[derive(Serialize, Deserialize, Clone, Key, Debug)]
// pub struct HostData {
//     pub value: String,
//     pub target: Option<PlayerId>,
// }

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub enum HostMsg {
    Data(Data, Option<PlayerId>),
    Kick(PlayerId),
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Data {
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Close {}
