@0xad530d6f7666fafb;

# A client sends some data to the server.
# This event is dispatched by a client.
struct ClientMessage {
    data @0 :Data;
}

# Msg from client with specific client in mind.
struct ToClient {
    clientId @0 :UInt64;
    data @1 :Data;
}

# Msg from the host.
# To be distributed.
struct HostMessage {
    data @0 :Data;
}

# Turn from the client, this is what a game reactor should react to.
struct ClientTurn {
    clientId @0 :UInt64;
    union {
        timeout @1 :Void;
        turn @2 :Data;
    }
}

# List of ClientTurns, this is used in step locked systems
# Then a game reactor should react to this
struct ClientStep {
    data @0 :List(ClientTurn);
}

# A struct was received from a client.
# This event is emitted by a client handler when it recieves a ClientSend.
struct FromClient {
    clientId @0 :UInt64;
    data @1 :Data;
}

# client message only for internal use in the ClientControllerReactor
# TODO: This should be gone, but things are hard ...
struct InnerToClient {
    data @0 :Data;
}
