@0xad530d6f7666fafb;

# A client sends some data to the server.
# This event is dispatched by a client.
struct ClientSend {
    data @0 :Text;
}

# A struct was received from a client.
# This event is emitted by a client handler when it recieves a ClientSend.
struct ClientMessage {
    clientId @0 :UInt64;
    data @1 :Text;
}

# A client was disconnected.
# Emitted by the client handler.
struct ClientDisconnected {
    clientId @0 :UInt64;
}

# A client has connected.
# Emitted by the client handler.
struct ClientConnected {
    clientId @0 :UInt64;
}
