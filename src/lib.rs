#![allow(dead_code)]
#![recursion_limit = "512"]

extern crate async_std;

extern crate bytes;
extern crate hex;

#[macro_use]
extern crate futures;
extern crate rand;

extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate tracing;
extern crate tracing_futures;

extern crate ws;
extern crate tokio_tungstenite;
extern crate ws_stream_tungstenite;

#[macro_use]
extern crate mozaic_derive;

pub mod modules;

pub mod generic;
pub mod graph;
