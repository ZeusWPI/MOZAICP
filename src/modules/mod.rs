
use messaging::reactor::*;
use messaging::types::*;

use log_capnp::{log};

mod cmd_reactor;
mod server_reactor;
pub use self::cmd_reactor::CmdReactor;
pub use self::server_reactor::ServerReactor;

mod logging;
pub use self::logging::{LogReactor, Link as LogLink};


// TODO: Make this really random ...
static LOGGER_ID: &'static [u8] = &[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31];



/// Get the id of the current logger.
pub fn logger_id() -> ReactorId {
    return ReactorId::from(LOGGER_ID);
}

/// Function to log with a reactor handle.
pub fn log_reactor<C: Ctx>(
    handle: &mut ReactorHandle<C>,
    msg: &str,
) {
    let mut joined = MsgBuffer::<log::Owned>::new();
    joined.build(|b| {
        b.set_log(&msg);
    });
    handle.send_internal(joined);
}

/// Function to log with a link handle.
pub fn log_handle<C: Ctx>(
    handle: &mut LinkHandle<C>,
    msg: &str,
) {
    let mut joined = MsgBuffer::<log::Owned>::new();
    joined.build(|b| {
        b.set_log(&msg);
    });
    handle.send_internal(joined);
}