mod handle;
mod params;
mod reactor;

pub use handle::ReactorHandle;
pub use params::CoreParams;
pub use reactor::{Reactor, ReactorState};

use super::{ReactorID};

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum TargetReactor {
    All,
    Reactor,
    Links,
    Link(ReactorID),
}
