use serde::{Deserialize, Serialize};

pub type PlayerId = u64;
pub type DataType = String;

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

impl HostMsg {
    pub fn new(value: DataType, player: Option<PlayerId>) -> Self {
        HostMsg::Data(Data { value }, player)
    }

    pub fn kick(player: PlayerId) -> Self {
        HostMsg::Kick(player)
    }
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Data {
    pub value: DataType,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Close {}
#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Start;
