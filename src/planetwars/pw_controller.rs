use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::mem;

use tokio;
use futures::{Future, Poll, Async, Stream};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};

use utils::request_handler::{
    ConnectionId,
    Event,
    EventContent,
    ConnectionHandle,
    ConnectionHandler,
    Command as ConnectionCommand,
    ResponseValue,
    ResponseError,
};
use network::router::RoutingTable;

use super::Config;
use super::pw_rules::{PlanetWars, Dispatch};
use super::pw_serializer::{serialize, serialize_rotated};
use super::pw_protocol::{
    self as proto,
    PlayerAction,
    PlayerCommand, 
    CommandError,
};

use slog;
use serde_json;

// TODO: find a better place for this
#[derive(PartialEq, Clone, Copy, Eq, Hash, Serialize, Deserialize, Debug)]
pub struct PlayerId {
    id: usize,
}

impl PlayerId {
    pub fn new(id: usize) -> PlayerId {
        PlayerId {
            id
        }
    }

    pub fn as_usize(&self) -> usize {
        self.id
    }
}

impl slog::KV for PlayerId {
    fn serialize(&self,
                 _record: &slog::Record,
                 serializer: &mut slog::Serializer)
                 -> slog::Result
    {
        serializer.emit_usize("player_id", self.as_usize())
    }
}

pub struct Player {
    id: PlayerId,
    connection: ConnectionHandle,
}

pub struct Lobby {
    conf: Config,
    logger: slog::Logger,

    connection_player: HashMap<ConnectionId, PlayerId>,
    players: HashMap<PlayerId, Player>,
}

impl Lobby {
    fn new(conf: Config,
           client_tokens: Vec<Vec<u8>>,
           routing_table: Arc<Mutex<RoutingTable>>,
           event_channel_handle: UnboundedSender<Event>,
           logger: slog::Logger)
           -> Self
    {
        let mut players = HashMap::new();
        let mut connection_player = HashMap::new();

        for (num, token) in client_tokens.into_iter().enumerate() {
            let player_id = PlayerId::new(num);
            let connection_id = ConnectionId { connection_num: num };

            let (handle, handler) = ConnectionHandler::new(
                connection_id,
                token,
                routing_table.clone(),
                event_channel_handle.clone(),
            );

            let player = Player {
                id: player_id,
                connection: handle,
            };

            tokio::spawn(handler);

            players.insert(player_id, player);
            connection_player.insert(connection_id, player_id);
        }

        return Lobby {
            conf,
            logger,
            connection_player,
            players,
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event.content {
            EventContent::Connected => {},
            EventContent::Disconnected => {},
            EventContent::Message { .. } => {},
            EventContent::Response { .. } => {},
        }
    }
}

pub struct PwController {
    state: PlanetWars,
    planet_map: HashMap<String, usize>,
    logger: slog::Logger,

    connection_player: HashMap<ConnectionId, PlayerId>,
    players: HashMap<PlayerId, Player>,

    waiting_for: HashSet<PlayerId>,
    commands: HashMap<PlayerId, ResponseValue>,

    event_channel: UnboundedReceiver<Event>,
}

impl PwController {
    pub fn new(lobby: Lobby) -> Self {
        let (snd, rcv) = mpsc::unbounded();

        let state = lobby.conf.create_game(lobby.players.len());

        let planet_map = state.planets.iter().map(|planet| {
            (planet.name.clone(), planet.id)
        }).collect();

        let mut controller = PwController {
            event_channel: rcv,
            state,
            planet_map,
            players: lobby.players,
            connection_player: lobby.connection_player,
            logger: lobby.logger,

            waiting_for: HashSet::new(),
            commands: HashMap::new(),
        };
        controller.start_game();
        return controller;
    }


    fn start_game(&mut self) {
        self.log_state();
        self.prompt_players();
    }

    /// Advance the game by one turn.
    fn step(&mut self, messages: HashMap<PlayerId, ResponseValue>) {
        self.state.repopulate();
        self.execute_messages(messages);
        self.state.step();

        self.log_state();

        if self.state.is_finished() {
        } else {
            self.prompt_players();
        }
    }

    fn outcome(&self) -> Option<Vec<PlayerId>> {
        if self.state.is_finished() {
            Some(self.state.living_players())
        } else {
            None
        }
    }

    fn log_state(&self) {
        // TODO: add turn number
        info!(self.logger, "step"; serialize(&self.state));
    }

    fn prompt_players(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);

        for player in self.state.players.iter() {
            if player.alive {
                let offset = self.state.players.len() - player.id.as_usize();

                let serialized_state = serialize_rotated(&self.state, offset);
                let message = proto::ServerMessage::GameState(serialized_state);
                let serialized = serde_json::to_vec(&message).unwrap();

                self.waiting_for.insert(player.id);
                self.players.get_mut(&player.id).unwrap().connection
                    .request(serialized, deadline);
            }
        }
    }

    fn execute_messages(&mut self, mut msgs: HashMap<PlayerId, ResponseValue>) {
        for (player_id, result) in msgs.drain() {
            // log received message
            // TODO: this should probably happen in the lock, so that
            //       we have a correct timestamp.
            match &result {
                &Ok(ref message) => {
                    let content = match String::from_utf8(message.clone()) {
                        Ok(content) => content,
                        Err(_err) => "invalid utf-8".to_string(),
                    };
                    info!(self.logger, "message received";
                        player_id,
                        "content" => content,
                    );
                },
                &Err(ResponseError::Timeout) => {
                    info!(self.logger, "timeout"; player_id);
                }
            }

            let player_action = self.execute_action(player_id, result);
            let message = proto::ServerMessage::PlayerAction(player_action);
            let serialized = serde_json::to_vec(&message).unwrap();
            self.players.get_mut(&player_id).unwrap().connection.
                send(serialized);
        }
    }

    fn execute_action(&mut self, player_id: PlayerId, response: ResponseValue)
        -> PlayerAction
    {
        // TODO: it would be cool if this could be done with error_chain.

        let message = match response {
            Err(ResponseError::Timeout) => return PlayerAction::Timeout,
            Ok(message) => message,
        };

        let action: proto::Action = match serde_json::from_slice(&message) {
            Err(err) => return PlayerAction::ParseError(err.to_string()),
            Ok(action) => action,
        };

        let commands = action.commands.into_iter().map(|command| {
            match self.parse_command(player_id, &command) {
                Ok(dispatch) => {
                    self.state.dispatch(&dispatch);
                    PlayerCommand {
                        command,
                        error: None,
                    }
                },
                Err(error) => {
                    PlayerCommand {
                        command,
                        error: Some(error),
                    }
                }
            }
        }).collect();

        return PlayerAction::Commands(commands);
    }

    fn parse_command(&self, player_id: PlayerId, mv: &proto::Command)
                     -> Result<Dispatch, CommandError>
    {
        let origin_id = *self.planet_map
            .get(&mv.origin)
            .ok_or(CommandError::OriginDoesNotExist)?;

        let target_id = *self.planet_map
            .get(&mv.destination)
            .ok_or(CommandError::DestinationDoesNotExist)?;

        if self.state.planets[origin_id].owner() != Some(player_id) {
            return Err(CommandError::OriginNotOwned);
        }

        if self.state.planets[origin_id].ship_count() < mv.ship_count {
            return Err(CommandError::NotEnoughShips);
        }

        if mv.ship_count == 0 {
            return Err(CommandError::ZeroShipMove);
        }

        Ok(Dispatch {
            origin: origin_id,
            target: target_id,
            ship_count: mv.ship_count,
        })
    }
}

impl Future for PwController {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            if self.state.is_finished() {
                return Ok(Async::Ready(()));
            }

            let event = try_ready!(self.event_channel.poll())
                .expect("event channel closed");

            match event.content {
                EventContent::Connected => {},
                EventContent::Disconnected => {},
                EventContent::Message { .. } => {},
                EventContent::Response { request_id, value } => {
                    // we only send requests to players
                    let &player_id = self.connection_player
                        .get(&event.connection_id).unwrap();
                    self.commands.insert(player_id, value);
                    self.waiting_for.remove(&player_id);
                }
            }

            if self.waiting_for.is_empty() {
                let commands = mem::replace(&mut self.commands, HashMap::new());
                self.step(commands);
            }
        }
    }
}