

use super::types::*;
mod types;
pub use types::Register;

pub mod client_controller;

/// So, networking and things
///
/// Registering is the act of sending a specific client key.
/// This client key is mapped to a client controller (ID'ed with ReactorID).
///
/// But who keeps the relations client_key -> ReactorID?
/// The client manager.
///
/// So you have a ConnectionManager, listening to tcp streams,
/// when the client identifies it may get a ReactorID, or get dropped.
/// Because of the modularity ConnectionManagers can be built with tcp streams,
/// udp streams, ws streams whatever, as long as they can build SenderHandles.
///
/// This way you can also use GameManagers that register expected connecting clients.
/// And when ClientControllers get dropped, they can unregister their client.
///
///                                            +--------------+<------------+------------------+
/// +----------+--spawn----------------------->| ClientStream |             | ClientController |
/// | TCP ConM |--1-----v                      +--------------+------------>+------------------+
/// +----------+        +---------------+<----------------------------------+ +------+     |
///          ^-------2--| ClientManager |<------------------------------------| Game |<----+
///          v-------2--+---------------+<----------------------------------+ +------+
/// +---------+         ^                                                   |   ^
/// | WS ConM |--1------+                      +--------------+<------------+---+--------------+
/// +---------+--spawn------------------------>| ClientStream |             | ClientController |
///                                            +--------------+------------>+------------------+
///
mod client_manager;
pub use client_manager::{ClientManager, PlayerUUIDs, RegisterGame, SpawnPlayer};

mod tcp_endpoint;
pub use tcp_endpoint::TcpEndpoint;

// / GameManager
// / ClientManager
// / ClientController

// TODO: add better tracing
