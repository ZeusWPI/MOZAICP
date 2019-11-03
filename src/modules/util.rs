use std::ops::Deref;

#[derive(Clone, Debug, Hash, Eq, PartialEq, Copy)]
pub struct Identifier(u64);

impl From<u64> for Identifier {
    fn from(src: u64) -> Identifier {
        Identifier(src)
    }
}

impl Into<u64> for Identifier {
    fn into(self) -> u64 {
        self.0
    }
}

impl Deref for Identifier {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Copy)]
pub struct PlayerId(u64);

impl From<u64> for PlayerId {
    fn from(src: u64) -> Self {
        PlayerId(src)
    }
}

impl Into<u64> for PlayerId {
    fn into(self) -> u64 {
        self.0
    }
}

impl Into<u64> for &PlayerId {
    fn into(self) -> u64 {
        self.0
    }
}

impl Deref for PlayerId {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}
