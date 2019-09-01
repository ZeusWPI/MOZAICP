@0xad560d4e7666ffff;

struct PwState {
    turn @0: UInt64;
    state @1: Text;
}

struct PwMove {
    userId @0: UInt64;
    move @1: Text;
}
