use futures::executor::ThreadPool;
// use futures::*;

use std::hash::Hash;
use std::collections::HashMap;

use crate::generic::*;
pub mod types;

mod controller;
pub use controller::ClientController;

pub type PlayerMap<P> = HashMap<types::PlayerId, P>;

pub struct ConnectionManager<K, M, P> {
    pool: ThreadPool,
    target: ReactorID,
    broker: BrokerHandle<K, M>,
    player_map: PlayerMap<P>,
}

impl<K, M, P> ConnectionManager<K, M, P>
where
    K: 'static + Hash + Eq,
    M: 'static,
{
    pub fn new(
        pool: ThreadPool,
        target: ReactorID,
        broker: BrokerHandle<K, M>,
        player_map: PlayerMap<P>,
    ) -> CoreParams<Self, K, M> {
        // Start listening for connections

        //
        let params = CoreParams::new(Self {
            pool,
            target,
            broker,
            player_map,
        });



        params
    }
}

impl<P> ReactorState<String, JSONMessage> for ConnectionManager<String, JSONMessage, P> {
    fn init<'a>(&mut self, _handle: &mut ReactorHandle<'a, String, JSONMessage>) {
        // spawn reactor like with the actually accepting

        // Open ConnectorLink to that reactor lik

    }
}

struct ConnectorLink {}

impl ConnectorLink {
    fn params() -> LinkParams<Self, String, JSONMessage> {
        let mut params = LinkParams::new(Self{});

        params.external_handler(FunctionHandler::from(Self::handle_register));
        params.internal_handler(FunctionHandler::from(Self::handle_accept));

        return params;
    }

    fn handle_register(&mut self, handle: &mut LinkHandle<String, JSONMessage>, register: &types::Register) {
        handle.send_internal(Typed::from(*register));
    }

    fn handle_accept(&mut self, handle: &mut LinkHandle<String, JSONMessage>, register: &types::Register) {
        handle.send_internal(Typed::from(*register));
    }
}
