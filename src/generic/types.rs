
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TypeID(u64);

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ReactorID(u64);

impl From<u64> for TypeID {
    fn from(id: u64) -> Self {
        TypeID(id)
    }
}

impl From<u64> for ReactorID {
    fn from(id: u64) -> Self {
        ReactorID(id)
    }
}

impl Into<u64> for TypeID {
    fn into(self) -> u64 {
        self.0
    }
}

impl Into<u64> for ReactorID {
    fn into(self) -> u64 {
        self.0
    }
}

use std::ops::Deref;
impl Deref for TypeID {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}

impl Deref for ReactorID {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}
