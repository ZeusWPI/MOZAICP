@0xad560d4e7436ffff;

# A turn time-out has occured.
# Note that this does not mean the turn timed out
# (it could already have passed)
struct TurnTimeout {
    turnNum @0 :UInt64;
}

# The game has stepped to a new state.
# This event is also sent to all living players to signal that a new turn
# has begun.
struct GameStep {
    turnNum @0 :UInt64;
    state @1 :Text;
}

# The game has finished.
# Sent as a 'final state' before disconnecting a client, both when the match
# ends or when a player dies (which is the 'match end' for that player).
struct GameFinished {
    turnNum @0 :UInt64;
    state @1 :Text;
}

# Noties a client of the result of the action it took.
struct PlayerAction {
    clientId @0 :UInt32;
    action @1 :Text;
}
