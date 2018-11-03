extern crate prost_build;
extern crate capnpc;

fn main() {
    prost_build::compile_protos(
        &["../proto/core.proto"],
        &["../proto"]
    ).unwrap();
    capnpc::CompilerCommand::new()
        .src_prefix("../schema")
        .file("../schema/core.capnp")
        .run().expect("schema compiler command");
}