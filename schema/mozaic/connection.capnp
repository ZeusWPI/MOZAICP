@0xed53776f7666fafb;

# A client was disconnected.
# Emitted by the client handler.
struct ClientDisconnected {
    clientKey @0 :UInt64;
}

# A client has connected.
# Emitted by the client handler.
struct ClientConnected {
    clientKey @0 :UInt64;
    id @1 :Data;
}

# The host has connected to the client
# Response on ClientConnected
struct HostConnected {
    clientKey @0 :UInt64;
    id @1 :Data;
}

# Server is tired of this client... :/
struct ClientKicked {
    id @0 :UInt64;
}
