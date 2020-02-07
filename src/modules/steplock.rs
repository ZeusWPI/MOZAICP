use super::types::{HostMsg, PlayerId, PlayerMsg};
use crate::generic::*;

use futures::channel::mpsc;

use tokio::time::delay_for;

use std::collections::HashMap;
use std::{any, mem};
use std::time::{Duration};

#[derive(Clone, Debug)]
struct TimeOut;

pub struct StepLock {
    step: HashMap<PlayerId, Option<PlayerMsg>>,
    players: Vec<PlayerId>,
    host: ReactorID,
    player_id: ReactorID,
    timeout_ms: Option<u64>,
    // Maybe add threadpool
}

impl StepLock {
    pub fn params(
        host: ReactorID,
        player_id: ReactorID,
        players: Vec<PlayerId>,
        timeout_ms: Option<u64>,
    ) -> CoreParams<Self, any::TypeId, Message> {
        CoreParams::new(Self {
            host,
            player_id,
            step: HashMap::new(),
            players,
            timeout_ms,
        })
        .handler(FunctionHandler::from(Self::player_msg))
        .handler(FunctionHandler::from(Self::timeout))
    }

    /// Insert the player message in the buffered message
    fn player_msg(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, e: &PlayerMsg) {
        self.step.insert(e.id, Some(e.clone()));
        if self.step.values().all(Option::is_some) {
            self.flush_msgs(handle);
        }
    }

    fn timeout(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, _e: &TimeOut) {
        self.flush_msgs(handle);
    }

    /// Flush all messages to the host
    fn flush_msgs(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>) {
        for (&id, msg) in self.step.iter_mut() {
            let msg = mem::replace(msg, None).unwrap_or(PlayerMsg {
                value: String::new(),
                id,
            });
            handle.send_internal(msg, TargetReactor::Link(self.host));
        }
    }
}

impl ReactorState<any::TypeId, Message> for StepLock {
    const NAME: &'static str = "StepLock";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        // Open link to host
        let host_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), PlayerMsg>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), HostMsg>(
                TargetReactor::Link(self.player_id),
            )));
        handle.open_link(self.host, host_link_params, true);

        // Open link to client
        let client_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), HostMsg>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), PlayerMsg>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.player_id, client_link_params, true);
        // Start timing out
        // Don't forget to restart timeout at send
        let timeout_id = ReactorID::rand();
        let (tx, rx) = mpsc::unbounded();
        let self_send_f = handle.chan();

        let timeout_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), TimeOut>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), TimeOut>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.player_id, timeout_params, true);

        handle.open_reactor_like(timeout_id, tx);

        let timeout_ms = self.timeout_ms;

        tokio::spawn(
            async move {
                let mut rx = receiver_handle(rx).boxed().fuse();

                if let Some(timeout_ms) = timeout_ms {
                    loop {
                        let (handle, timeout) = create_timer(timeout_ms);
                        let mut timeout = timeout.fuse();
    
                        select! {
                            v = rx.next() => {
                                if v.is_none() {
                                    break;
                                }
                                handle.abort();
                                self_send_f.send(timeout_id, TimeOut)?;
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
            }
        );
    }
}


use futures::future::{Abortable, AbortHandle, Future};
use futures::{FutureExt, StreamExt};

fn create_timer(time_ms: u64) -> (AbortHandle, impl Future<Output=()>) {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    (
        abort_handle,
        Abortable::new(delay_for(Duration::from_millis(time_ms)), abort_registration).map(|_| ()),
    )
}
