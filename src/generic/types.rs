use serde::{Serialize, Deserialize};

use std::ops::Deref;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactorID(u64);

impl ReactorID {
    pub fn rand() -> Self {
        rand::random::<u64>().into()
    }
}

impl From<u64> for ReactorID {
    fn from(id: u64) -> Self {
        ReactorID(id)
    }
}


impl Into<u64> for ReactorID {
    fn into(self) -> u64 {
        self.0
    }
}

impl Deref for ReactorID {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}
