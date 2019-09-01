extern crate capnpc;

fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("./schema")
        .file("./schema/core.capnp")
        .file("./schema/network.capnp")
        .file("./schema/chat.capnp")
        .file("./schema/planetwars.capnp")
        .file("./schema/client_events.capnp")
        .file("./schema/match_control.capnp")
        .file("./schema/match_events.capnp")
        .file("./schema/server_control.capnp")
        .file("./schema/my.capnp")
        .file("./schema/mozaic/cmd.capnp")
        .run().expect("schema compiler command");
}
