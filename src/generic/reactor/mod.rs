mod handle;
mod params;
mod reactor;

pub use handle::ReactorHandle;
pub use params::CoreParams;
pub use reactor::{Reactor, ReactorState};

use super::{LinkSpawner, ReactorID};

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
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
