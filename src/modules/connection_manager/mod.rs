
//!
//! Expected control flow:
//!
//! Connection manager
//! - Create client controllers for every expected party
//! - Listen for incoming connections
//! - For each connection
//!   + Open link with connecting party
//!   + Wait for them to identify
//!   + Pass them through to the correct client controller
//!   + You cannot close the link, because the connection manager is responsible for disconnecting the reactor
//!     (this is not wanted but currently a limitation of the MOZAIC implementation)
//! - When all client controllers are gone (the game has finished), kill yourself
//!
//! Client controller:
//! - Maintain link with Connection Manager
//! - Handle actor joined or something as client connected, then
//!   + Open link with the joined actor
//!   + On disconnect handle disconnect
//! - On host ended
//!   + Close link with Connection Manager
//!   + Close potential link with client
//!

pub mod client_controller;
mod connection_manager;

pub use self::connection_manager::ConnectionManager;
