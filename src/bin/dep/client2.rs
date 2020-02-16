extern crate capnp;
extern crate cursive;
extern crate futures;
extern crate hex;
extern crate mozaic;
extern crate rand;
extern crate tokio;

use mozaic::base_capnp::{client_message, host_message};
use mozaic::cmd_capnp::{bot_input, bot_return};
use mozaic::connection_capnp::client_kicked;
use mozaic::core_capnp::{actor_joined, identify, initialize, terminate_stream};
use mozaic::errors::*;
use mozaic::messaging::reactor::*;
use mozaic::messaging::types::*;
use mozaic::modules::BotReactor;
use mozaic::runtime::{self, Broker, BrokerHandle};

use rand::Rng;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let id = args.get(1).unwrap().parse().unwrap();

    let addr = "127.0.0.1:9142".parse().unwrap();
    let self_id: ReactorId = rand::thread_rng().gen();

    tokio::run(futures::lazy(move || {
        let mut broker = Broker::new().unwrap();
        let reactor = ClientReactor {
            server: None,
            id,
            broker: broker.clone(),
        };
        broker
            .spawn(self_id.clone(), reactor.params(), "main")
            .display();

        tokio::spawn(runtime::connect_to_server(broker, self_id, &addr));

        Ok(())
    }));
}

// Main client logic
/// ? greeter_id is the server, the tcp stream that you connected to
/// ? runtime_id is your own runtime, to handle visualisation etc
/// ? user are you  ...
struct ClientReactor {
    server: Option<ReactorId>,
    id: u64,
    broker: BrokerHandle,
}

impl ClientReactor {
    fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::initialize));
        params.handler(actor_joined::Owned, CtxHandler::new(Self::open_host));
        return params;
    }

    // reactor setup
    fn initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()> {
        let runtime_link = RuntimeLink::params(handle.id().clone());
        handle.open_link(runtime_link)?;

        let bot = BotReactor::new(
            self.broker.clone(),
            handle.id().clone(),
            vec!["bash".to_string(), "reader.sh".to_string()],
        );
        let bot_id = handle.spawn(bot.params(), "Bot Driver")?;

        handle.open_link(BotLink::params(bot_id))?;

        return Ok(());
    }

    fn open_host<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()> {
        let id = r.get_id()?;

        if let Some(server) = &self.server {
            handle.open_link(HostLink::params(ReactorId::from(id)))?;
            self.broker.register_as(id.into(), server.clone(), "Host");
        } else {
            handle.open_link(ServerLink::params(id.into()))?;
            self.server = Some(id.into());

            let mut identify = MsgBuffer::<identify::Owned>::new();
            identify.build(|b| {
                b.set_key(self.id);
            });
            handle.send_internal(identify).display();
        }

        Ok(())
    }
}

// Handler for the connection with the chat server
struct ServerLink;
impl ServerLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);

        params.external_handler(
            terminate_stream::Owned,
            CtxHandler::new(Self::close_handler),
        );

        params.external_handler(
            actor_joined::Owned,
            CtxHandler::new(Self::handle_actor_joined),
        );

        params.internal_handler(identify::Owned, CtxHandler::new(Self::identify));

        return params;
    }

    fn handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        id: actor_joined::Reader,
    ) -> Result<()> {
        let id = id.get_id()?;

        let mut joined = MsgBuffer::<actor_joined::Owned>::new();
        joined.build(|b| b.set_id(id));
        handle.send_internal(joined)?;

        Ok(())
    }

    fn identify<C: Ctx>(&mut self, handle: &mut LinkHandle<C>, id: identify::Reader) -> Result<()> {
        let id = id.get_key();

        let mut chat_message = MsgBuffer::<identify::Owned>::new();
        chat_message.build(|b| {
            b.set_key(id);
        });

        handle.send_message(chat_message).display();
        Ok(())
    }

    fn close_handler<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: terminate_stream::Reader,
    ) -> Result<()> {
        // also close our end of the stream
        handle.close_link()?;
        return Ok(());
    }
}

struct HostLink;
impl HostLink {
    fn params<C: Ctx>(remote_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(remote_id, HostLink);

        params.external_handler(
            host_message::Owned,
            CtxHandler::new(Self::receive_chat_message),
        );

        params.internal_handler(bot_return::Owned, CtxHandler::new(Self::send_chat_message));

        params.external_handler(client_kicked::Owned, CtxHandler::new(Self::client_kicked));

        return params;
    }

    // pick up a 'send_message' event from the reactor, and put it to effect
    // by constructing the chat message and sending it to the chat server.
    fn send_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        send_message: bot_return::Reader,
    ) -> Result<()> {
        let message = send_message.get_message()?;

        let mut chat_message = MsgBuffer::<client_message::Owned>::new();
        chat_message.build(|b| {
            b.set_data(message);
        });

        handle.send_message(chat_message)?;

        return Ok(());
    }

    // pick up a 'send_message' event from the reactor, and put it to effect
    // by constructing the chat message and sending it to the chat server.
    fn client_kicked<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: client_kicked::Reader,
    ) -> Result<()> {
        let mut chat_message = MsgBuffer::<host_message::Owned>::new();
        chat_message.build(|b| {
            b.set_data(b"You got kicked");
        });
        handle.send_internal(chat_message)?;

        let chat_message = MsgBuffer::<client_kicked::Owned>::new();
        handle.send_internal(chat_message)?;

        handle.close_link()?;

        return Ok(());
    }

    // receive a chat message from the chat server, and broadcast it on the
    // reactor.
    fn receive_chat_message<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        chat_message: host_message::Reader,
    ) -> Result<()> {
        let message = chat_message.get_data()?;

        let mut chat_message = MsgBuffer::<bot_input::Owned>::new();
        chat_message.build(|b| {
            b.set_input(message);
        });
        handle.send_internal(chat_message)?;

        return Ok(());
    }
}

struct BotLink;
impl BotLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);

        params.external_handler(bot_return::Owned, CtxHandler::new(bot_return::e_to_i));
        params.internal_handler(bot_input::Owned, CtxHandler::new(bot_input::i_to_e));

        return params;
    }
}

//
struct RuntimeLink;

impl RuntimeLink {
    fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);

        params.external_handler(actor_joined::Owned, CtxHandler::new(actor_joined::e_to_i));

        return params;
    }
}
