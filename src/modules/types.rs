use crate::generic::ReactorID;
use serde::{Deserialize, Serialize};

pub type Uuid = uuid::Uuid;
pub type PlayerId = u64;
pub type DataType = Vec<u8>;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct PlayerMsg {
    pub id: PlayerId,
    pub data: Option<Data>,
}

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
pub struct Start {
    pub players: Vec<(PlayerId, String)>,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub enum ClientState {
    Connected,
    Disconnected,
}
#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct ClientStateUpdate {
    pub id: PlayerId,
    pub state: ClientState,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct NewClientController(pub PlayerId, pub ReactorID);
#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct ClientControllerDisconnect(pub PlayerId);
