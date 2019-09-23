@0xd39ef24540211376;

# this should be removed, this entire file, it does not belong here...
struct SendMessage {
    user @0: Text;
    message @1: Text;
}

struct ChatMessage {
    user @0: Text;
    message @1: Text;
}

struct ConnectToGui {}

struct UserInput {
    text @0: Text;
}
