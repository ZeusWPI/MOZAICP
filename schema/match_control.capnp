@0xad560d4f7666ffff;

struct CreateClientRequest {
    publicKey @0 :Data;
}

struct CreateClientResponse {
    clientId @0 :UInt32;
}

# Add a player to the game.
struct RegisterClient {
    clientId @0 :UInt32;
    publicKey @1 :Data;
}

# Remove a player from the game.
struct RemoveClient {
    clientId @0 :UInt32;
}

# Start the game.
struct StartGame {
    mapPath @0 :Text;
    maxTurns @1 :UInt64;
}

# It's a match event!
struct MatchEvent {
    timestamp @0 :UInt64;
    typeId @1 :UInt32;
    data @2 :Data;
}
