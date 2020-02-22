use super::types::{Data, HostMsg, PlayerId, PlayerMsg};
use crate::generic::*;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::{FutureExt, StreamExt};

use tokio::time::delay_for;

use std::collections::HashMap;
use std::time::Duration;
use std::{any, mem};

#[derive(Clone, Debug)]
struct TimeOut;

#[derive(Clone, Debug)]
struct ResetTimeOut;

pub struct StepLock {
    step: HashMap<PlayerId, Option<Data>>,
    players: Vec<PlayerId>,
    host: ReactorID,
    player_id: ReactorID,
    timeout_ms: Option<Duration>,
    init_timeout_ms: Option<Duration>,
    tp: ThreadPool,
}

impl StepLock {
    pub fn new(players: Vec<PlayerId>, tp: ThreadPool) -> Self {
        Self {
            host: 0.into(),
            player_id: 0.into(),
            step: players.iter().map(|&id| (id, None)).collect(),
            players,
            timeout_ms: None,
            init_timeout_ms: None,
            tp,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_ms = Some(timeout);
        self
    }

    pub fn with_init_timeout(mut self, timeout: Duration) -> Self {
        self.init_timeout_ms = Some(timeout);
        self
    }

    pub fn params(
        mut self,
        host: ReactorID,
        player_id: ReactorID,
    ) -> CoreParams<Self, any::TypeId, Message> {
        self.host = host;
        self.player_id = player_id;
        CoreParams::new(self)
            .handler(FunctionHandler::from(Self::player_msg))
            .handler(FunctionHandler::from(Self::timeout))
    }

    /// Insert the player message in the buffered message
    fn player_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, e: &PlayerMsg) {
        info!("Got player data");
        self.step.insert(
            e.id,
            Some(
                e.data
                    .as_ref()
                    .expect("Player msg from player without DATA?")
                    .clone(),
            ),
        );
        if self.step.values().all(Option::is_some) {
            self.flush_msgs(handle);
        }
    }

    fn timeout(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, _e: &TimeOut) {
        self.flush_msgs(handle);
    }

    /// Flush all messages to the host
    fn flush_msgs(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>) {
        handle.send_internal(ResetTimeOut, TargetReactor::Links);
        for (&id, msg) in self.step.iter_mut() {
            let msg = PlayerMsg {
                id,
                data: mem::replace(msg, None),
            };
            handle.send_internal(msg, TargetReactor::Link(self.host));
        }
    }
}

use crate::modules::game_manager::types::*;
impl ReactorState<any::TypeId, Message> for StepLock {
    const NAME: &'static str = "StepLock";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        // Open link to host
        let host_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), PlayerMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), StateRes>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), HostMsg>(
                TargetReactor::Link(self.player_id),
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), StateReq>(
                TargetReactor::Link(self.player_id),
            )));
        handle.open_link(self.host, host_link_params, true);

        // Open link to client
        let client_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), HostMsg>()))
            .internal_handler(FunctionHandler::from(i_to_e::<(), StateReq>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), PlayerMsg>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), StateRes>(
                TargetReactor::Link(self.host),
            )));
        handle.open_link(self.player_id, client_link_params, true);

        // Start timing out
        let timeout_id = ReactorID::rand();
        let (tx, rx) = mpsc::unbounded();
        let self_send_f = handle.chan();

        let timeout_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), ResetTimeOut>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), TimeOut>(
                TargetReactor::Reactor,
            )));
        handle.open_link(timeout_id, timeout_params, true);

        let timeout_ms = self.timeout_ms.clone();
        let init_timeout = self.init_timeout_ms.clone();

        let fut = async move {
            let mut rx = receiver_handle(rx).boxed().fuse();

            if let Some(init_timeout) = init_timeout {
                let mut timeout = delay_for(init_timeout).fuse();
                select! {
                    v = rx.next() => {
                    },
                    v = timeout => {
                        self_send_f.send(timeout_id, TimeOut)?;
                    }
                }
            }

            if let Some(timeout_ms) = timeout_ms {
                loop {
                    let mut timeout = delay_for(timeout_ms).fuse();
                    select! {
                        v = rx.next() => {
                            if v.is_none() {
                                break;
                            }
                        },
                        v = timeout => {
                            self_send_f.send(timeout_id, TimeOut)?;
                        }
                    }
                }
            } else {
                loop {
                    let v = rx.next().await;
                    if v.is_none() {
                        break;
                    }
                    self_send_f.send(timeout_id, TimeOut)?;
                }
            }

            Some(())
        }.map(|_| ());

        handle.open_reactor_like(timeout_id, tx, fut, "Timeouter");
    }
}
