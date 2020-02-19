use serde::{Deserialize, Serialize};

use super::PlayerId;
use crate::generic::ReactorID;

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Register {
    pub player: u64,
}

#[derive(Serialize, Deserialize, Clone, Key, Debug)]
pub struct Accepted {
    pub player: PlayerId,
    pub client_id: ReactorID,
    pub contr_id: ReactorID,
}
