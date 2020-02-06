
mod reactor;
mod handle;
mod params;

pub use reactor::{Reactor, ReactorState};
pub use handle::ReactorHandle;
pub use params::CoreParams;

use super::{ReactorID, LinkSpawner};

#[derive(Eq, PartialEq, Debug)]
pub enum TargetReactor {
    All,
    Reactor,
    Links,
    Link(ReactorID),
}

/// Inner op for reactors
pub enum InnerOp<K, M> {
    OpenLink(ReactorID, LinkSpawner<K, M>, bool),
    CloseLink(ReactorID),
    Close(),
}
