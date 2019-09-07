@0xad530d6f7666fafb;

# A client sends some data to the server.
# This event is dispatched by a client.
struct ClientMessage {
    data @0 :Text;
}

# A struct was received from a client.
# This event is emitted by a client handler when it recieves a ClientSend.
struct FromClient {
    clientId @0 :UInt64;
    data @1 :Text;
}

# client message only for internal use in the ClientControllerReactor
struct InnerToClient {
    data @0 :Text;
}

# Msg from client with specific client in mind.
struct ToClient {
    clientId @0 :UInt64;
    data @1 :Text;
}

# Msg from the host.
# To be distributed.
struct HostMessage {
    data @0 :Text;
}

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

struct HostConnected {
    clientKey @0 :UInt64;
    id @1 :Data;
}
