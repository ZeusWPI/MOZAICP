use super::types::{ClientStateUpdate, Data, HostMsg, PlayerId, PlayerMsg, Start};
use crate::generic::*;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::{FutureExt, StreamExt};

use async_std::task::sleep;

use std::collections::HashMap;
use std::time::Duration;
use std::{any, mem};

#[derive(Clone, Debug)]
struct TimeOut;

#[derive(Clone, Debug)]
struct ResetTimeOut;

#[derive(Clone)]
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
            .handler(FunctionHandler::from(Self::host_msg))
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

    fn host_msg(&mut self, _handle: &mut ReactorHandle<any::TypeId, Message>, m: &HostMsg) {
        if let HostMsg::Kick(id) = m {
            self.step.remove(&id);
        }
    }

    fn timeout(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>, _e: &TimeOut) {
        self.flush_msgs(handle);
    }

    /// Flush all messages to the host
    fn flush_msgs(&mut self, handle: &mut ReactorHandle<any::TypeId, Message>) {
        handle.send_internal(ResetTimeOut, TargetReactor::Links);
        let mut player_msgs = Vec::new();
        for (&id, msg) in self.step.iter_mut() {
            let msg = PlayerMsg {
                id,
                data: mem::replace(msg, None),
            };
            player_msgs.push(msg);
        }
        handle.send_internal(player_msgs, TargetReactor::Link(self.host));
    }
}

use crate::util::request::*;

impl ReactorState<any::TypeId, Message> for StepLock {
    const NAME: &'static str = "StepLock";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        // Open link to host
        let host_link_params = LinkParams::new(())
            .internal_handler(IToE::<PlayerMsg>::new())
            .internal_handler(IToE::<Vec<PlayerMsg>>::new())
            .internal_handler(IToE::<Res<State>>::new())
            .internal_handler(IToE::<ClientStateUpdate>::new())
            .internal_handler(IToE::<Start>::new())
            .external_handler(EToI::<HostMsg>::new(TargetReactor::All))
            .external_handler(EToI::<Req<State>>::new(TargetReactor::Link(self.player_id)));
        handle.open_link(self.host, host_link_params, true);

        // Open link to client
        let client_link_params = LinkParams::new(())
            .internal_handler(IToE::<HostMsg>::new())
            .internal_handler(IToE::<Req<State>>::new())
            .external_handler(EToI::<PlayerMsg>::new(TargetReactor::Reactor))
            .external_handler(EToI::<Start>::new(TargetReactor::Link(self.host)))
            .external_handler(EToI::<Res<State>>::new(TargetReactor::Link(self.host)))
            .external_handler(EToI::<ClientStateUpdate>::new(TargetReactor::Link(
                self.host,
            )));
        handle.open_link(self.player_id, client_link_params, true);

        // Start timing out
        let timeout_id = ReactorID::rand();
        let (tx, rx) = mpsc::unbounded();
        let self_send_f = handle.chan();

        let timeout_params = LinkParams::new(())
            .internal_handler(IToE::<ResetTimeOut>::new())
            .external_handler(EToI::<TimeOut>::new(TargetReactor::Reactor));
        handle.open_link(timeout_id, timeout_params, true);

        let timeout_ms = self.timeout_ms.clone();
        let init_timeout = self.init_timeout_ms.clone();

        let fut = async move {
            let mut rx = receiver_handle(rx).boxed().fuse();

            if let Some(init_timeout) = init_timeout {
                let mut timeout = sleep(init_timeout).boxed().fuse();
                select! {
                    v = rx.next() => {
                        v??;
                    },
                    v = timeout => {
                        self_send_f.send(timeout_id, TimeOut)?;
                    }
                }
            } else {
                rx.next().await??;
            }

            if let Some(timeout_ms) = timeout_ms {
                loop {
                    let mut timeout = sleep(timeout_ms).boxed().fuse();
                    select! {
                        v = rx.next() => {
                            if v?.is_none() {
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
                    let v = rx.next().await?;
                    if v.is_none() {
                        break;
                    }
                    self_send_f.send(timeout_id, TimeOut)?;
                }
            }

            Some(())
        }
        .map(|_| ());

        handle.open_reactor_like(timeout_id, tx, fut, "Time-out Generator");
    }
}
