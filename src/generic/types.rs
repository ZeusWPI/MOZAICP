use serde::{Deserialize, Serialize};

use std::ops::Deref;

use std::fmt;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactorID(u64);

impl ReactorID {
    pub fn rand() -> Self {
        rand::random::<u64>().into()
    }

    pub fn u64(&self) -> u64 {
        self.0
    }
}

/// Very descriptive
impl fmt::Display for ReactorID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
