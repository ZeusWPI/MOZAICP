use serde::{Deserialize, Serialize};

use super::PlayerId;
use crate::modules::types::Uuid;
use crate::generic::ReactorID;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Register {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Accepted {
    pub player: PlayerId,
    pub name: String,
    pub client_id: ReactorID,
    pub contr_id: ReactorID,
}
