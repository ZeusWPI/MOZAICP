#![allow(dead_code)]
#![feature(arbitrary_self_types)]

extern crate bytes;
extern crate hex;

extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;
extern crate tokio;
extern crate tokio_timer;
extern crate tokio_codec;
#[macro_use]
extern crate futures;
extern crate rand;

extern crate serde;
extern crate serde_json;
extern crate erased_serde;
#[macro_use]
extern crate error_chain;

extern crate serde_derive;

extern crate prost;
#[macro_use]
extern crate prost_derive;
extern crate sodiumoxide;
extern crate capnp;
extern crate capnp_futures;

pub mod messaging;
pub mod net;

pub mod server;
pub mod client;

pub mod modules;

pub mod core_capnp {
    include!(concat!(env!("OUT_DIR"), "/core_capnp.rs"));
}

pub mod chat_capnp {
    include!(concat!(env!("OUT_DIR"), "/chat_capnp.rs"));
}

pub mod my_capnp {
    include!(concat!(env!("OUT_DIR"), "/my_capnp.rs"));
}

pub mod network_capnp {
    include!(concat!(env!("OUT_DIR"), "/network_capnp.rs"));
}

pub mod match_control_capnp {
    include!(concat!(env!("OUT_DIR"), "/match_control_capnp.rs"));
}

pub mod mozaic_cmd_capnp {
    include!(concat!(env!("OUT_DIR"), "/mozaic/cmd_capnp.rs"));
}

pub mod log_capnp {
    include!(concat!(env!("OUT_DIR"), "/mozaic/logging_capnp.rs"));
}
