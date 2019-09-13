
use messaging::reactor::*;
use messaging::types::*;
use errors::{Result, Consumable};
use core_capnp::{initialize};
use server::runtime::{BrokerHandle};

use core_capnp::{timeout, set_timeout};
use client_capnp::{from_client, to_client, host_message, client_step};
use super::util::{PlayerId};

use std::collections::HashMap;
use std::convert::TryInto;

use tokio::sync::mpsc;

pub struct Steplock {
    broker: BrokerHandle,
    timeout: Option<u64>,
    msgs: HashMap<PlayerId, Vec<u8>>,
    players: Vec<PlayerId>,
    host_id: ReactorId,
    client_id: ReactorId,
    timer: Option<mpsc::Sender<TimerAction>>,
}

impl Steplock {
    pub fn new(broker: BrokerHandle, players: Vec<PlayerId>, host_id: ReactorId, client_id: ReactorId) -> Self {
        Self {
            broker,
            timeout: None,
            msgs: HashMap::new(),
            timer: None,
            players,
            host_id,
            client_id
        }
    }

    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(timeout::Owned, CtxHandler::new(Self::handle_timeout));
        params.handler(from_client::Owned, CtxHandler::new(Self::handle_from_client));
        params.handler(set_timeout::Owned, CtxHandler::new(Self::handle_set_timeout));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        handle.open_link(TimeoutLink::params(handle.id().clone()))?;

        handle.open_link(ClientsLink::params(self.client_id.clone()))?;
        handle.open_link(HostLink::params(self.host_id.clone()))?;

        if let Some(timeout) = self.timeout {
            // Open timeout shit
            self.timer = Some(
                Timer::new(self.broker.clone(), timeout, handle.id().clone())
            );
        }

        Ok(())
    }

    pub fn set_timout(&mut self) {
        self.timer.as_mut().map(|tx| tx.try_send(TimerAction::Reset).unwrap());
    }

    pub fn stop_timeout(&mut self) {
        self.timer.as_mut().map(|tx| tx.try_send(TimerAction::Halt).unwrap());
    }

    fn handle_set_timeout<C: Ctx>(
        &mut self,
        _handle: &mut ReactorHandle<C>,
        _: set_timeout::Reader,
    ) -> Result<()>
    {
        self.set_timout();

        Ok(())
    }

    fn flush<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
    ) -> Result<()> {
        let mut msgs = MsgBuffer::<client_step::Owned>::new();
        msgs.build(|b| {
            let mut data_list = b.reborrow().init_data(self.players.len().try_into().unwrap());

            for (i, player) in self.players.iter().enumerate() {
                let mut builder = data_list.reborrow().get(i.try_into().unwrap());
                builder.set_client_id((*player).into());

                if let Some(data) = self.msgs.get(player) {
                    builder.set_turn(data);
                } else {
                    builder.set_timeout(());
                }
            }
        });
        handle.send_internal(msgs)?;

        self.msgs.clear();
        self.stop_timeout();

        Ok(())
    }

    fn handle_timeout<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: timeout::Reader,
    ) -> Result<()>
    {
        println!("Handling timeout");
        self.flush(handle)?;

        Ok(())
    }

    fn handle_from_client<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: from_client::Reader,
    ) -> Result<()>
    {
        let id = r.get_client_id();
        let msg = r.get_data()?;

        self.msgs.insert(id.into(), msg.to_vec());

        if self.msgs.len() == self.players.len() {
            self.flush(handle)?;
        }

        Ok(())
    }
}

struct TimeoutLink;
impl TimeoutLink {
    fn params<C:Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, Self);
        params.external_handler(timeout::Owned, CtxHandler::new(timeout::e_to_i));
        params
    }
}

struct ClientsLink;
impl ClientsLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, Self);
        params.external_handler(from_client::Owned, CtxHandler::new(from_client::e_to_i));
        params.internal_handler(host_message::Owned, CtxHandler::new(host_message::i_to_e));
        params.internal_handler(to_client::Owned, CtxHandler::new(to_client::i_to_e));
        params
    }
}

struct HostLink;
impl HostLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, Self);
        params.internal_handler(client_step::Owned, CtxHandler::new(client_step::i_to_e));
        params.external_handler(host_message::Owned, CtxHandler::new(host_message::e_to_i));
        params.external_handler(to_client::Owned, CtxHandler::new(to_client::e_to_i));
        params.external_handler(set_timeout::Owned, CtxHandler::new(set_timeout::e_to_i));
        params
    }
}

use std::time::{Duration, Instant};

use tokio::prelude::{Stream};
use tokio::timer::Delay;
use futures::{Poll, Async};
use futures::future::Future;

struct Timer {
    inner: Option<Delay>,
    rx: mpsc::Receiver<TimerAction>,
    broker: BrokerHandle,
    timeout: u64,
    id: ReactorId,
}

impl Timer {
    fn new(broker: BrokerHandle, timeout: u64, id: ReactorId) -> mpsc::Sender<TimerAction> {
        let inner = None;
        let (tx, rx) = mpsc::channel(20);

        let me = Self {
            inner, rx, broker, timeout, id
        };

        tokio::spawn(me);

        tx
    }
}

impl Future for Timer {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Ok(Async::Ready(result)) = self.rx.poll() {
            match result {
                None => return Ok(Async::Ready(())),
                Some(action) => match action {
                    TimerAction::Reset => {
                        self.inner = Some(
                            Delay::new(Instant::now() + Duration::from_millis(self.timeout))
                        );
                        println!("RESETTING");
                    },
                    TimerAction::Halt => {
                        self.inner = None;
                        println!("Halting");
                    }
                }
            }
        }

        if let Some(Ok(Async::Ready(_))) = self.inner.as_mut().map(|future| future.poll()) {
            self.inner = None;
            self.broker.send_message(
                &self.id,
                &self.id,
                timeout::Owned,
                |_| { }
            ).display();

            println!("TIMEOUT");
        }

        Ok(Async::NotReady)
    }
}

#[derive(Debug, Clone)]
enum TimerAction {
    Reset,
    Halt,
}