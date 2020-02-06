
mod link;
mod handle;
mod params;

pub type Closer<S, K, M> = Box<dyn for<'a> Fn(&mut S, &mut LinkHandle<'a, K, M>) -> () + Send>;

pub use handle::LinkHandle;
pub use link::{Link, LinkState};
pub use params::LinkParams;
