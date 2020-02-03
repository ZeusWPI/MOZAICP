use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::generic::ReactorID;

pub type PlayerId = u64;

#[derive(Serialize, Deserialize, Clone, Copy, Key)]
pub struct Register {
    pub player: PlayerId,
}

#[derive(Serialize, Deserialize, Clone, Copy, Key)]
pub struct Accepted {
    pub player: PlayerId,
    pub target: ReactorID,
}

#[derive(Serialize, Deserialize, Key)]
pub struct Data {
    pub value: Value,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Close {}

#[derive(Serialize, Deserialize)]
pub struct PlayerMsg {
    pub value: Value,
    pub id: PlayerId,
}

trait WithLen {
    fn len(&self) -> usize;
}

impl<T> WithLen for Vec<T> {
    fn len(&self) -> usize {
        self.len()
    }
}

struct Wrapping<I> {
    inner: I,
}

impl<I> Wrapping<I> {
    fn from(inner: I) -> Self {
        Wrapping { inner }
    }
    fn inner(self) -> I {
        self.inner
    }
}

use std::ops;
impl<I: ops::Index<usize> + WithLen> ops::Index<usize> for Wrapping<I> {
    type Output = I::Output;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[index % self.inner.len()]
    }
}

impl<I: ops::IndexMut<usize> + WithLen> ops::IndexMut<usize> for Wrapping<I> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let len = self.inner.len();
        &mut self.inner[index % len]
    }
}

impl<I> ops::Deref for Wrapping<I> {
    type Target = I;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<I> ops::DerefMut for Wrapping<I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
