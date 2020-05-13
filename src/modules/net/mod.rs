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
use super::types::*;
mod types;
pub use types::Register;

pub mod client_controller;

mod client_manager;
pub use client_manager::{ClientManager, RegisterGame, SpawnCC, SpawnPlayer};

mod tcp_endpoint;
pub use tcp_endpoint::TcpEndpoint;

mod udp_endpoint;
pub use udp_endpoint::UdpEndpoint;

// mod ws_endpoint;
// pub use ws_endpoint::WSEndpoint;

use crate::generic::*;
use futures::future::Future;
use std::any;
use std::hash::Hash;
use std::pin::Pin;

pub trait EndpointBuilder {
    fn build(
        self,
        id: ReactorID,
        cm_chan: SenderHandle<any::TypeId, Message>,
    ) -> (
        Sender<any::TypeId, Message>,
        Pin<Box<dyn Future<Output = Option<()>> + Send>>,
    );
}

pub trait ClientControllerBuilder<K, M>
where
    K: 'static + Eq + Hash + Send + Unpin,
    M: 'static + Send,
{
    fn build<'a>(
        &self,
        spawn: SpawnHandle<'a, K, M>,
        cm_id: ReactorID,
    ) -> (Uuid, PlayerId, ReactorID, ReactorID);
}

// / GameManager
// / ClientManager
// / ClientController

// TODO: add better tracing
