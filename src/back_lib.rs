#![allow(dead_code)]

extern crate bytes;
extern crate hex;

extern crate tokio_process;
extern crate tokio_core;
extern crate tokio;
#[macro_use]
extern crate futures;
extern crate rand;

extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate error_chain;

extern crate serde_derive;

extern crate capnp;
extern crate capnp_futures;

extern crate mozaic_derive;

pub mod messaging;
pub mod net;

pub mod server;
pub mod client;

pub mod modules;

pub mod errors;

macro_rules! add_gen {
  ($(
    pub mod $name:ident {
      $($content:tt)*
    }
  )*) => {
    $(
      pub mod $name {

          pub fn e_to_i<C: ::messaging::reactor::Ctx, T>(
            _: &mut T,
            h: &mut ::messaging::reactor::LinkHandle<C>,
            r: Reader) -> ::errors::Result<()>
        {
            let m = ::messaging::types::MsgBuffer::<Owned>::from_reader(r)?;
            h.send_internal(m)?;
            Ok(())
        }

        pub fn i_to_e<C: ::messaging::reactor::Ctx, T>(
            _: &mut T,
            h: &mut ::messaging::reactor::LinkHandle<C>,
            r: Reader) -> ::errors::Result<()>
        {
            let m = ::messaging::types::MsgBuffer::<Owned>::from_reader(r)?;
            h.send_message(m)?;
            Ok(())
        }

        $($content)*
      }
    )*
  };
}

pub mod core_capnp {
    add_gen!(%%/core_capnp.rs%%);
}

pub mod chat_capnp {
    add_gen!(%%/chat_capnp.rs%%);
}

pub mod my_capnp {
    add_gen!(%%/my_capnp.rs%%);
}

pub mod network_capnp {
    add_gen!(%%/network_capnp.rs%%);
}

pub mod match_control_capnp {
    add_gen!(%%/match_control_capnp.rs%%);
}

pub mod mozaic_cmd_capnp {
    add_gen!(%%/mozaic/cmd_capnp.rs%%);
}

pub mod log_capnp {
    add_gen!(%%/mozaic/logging_capnp.rs%%);
}

pub mod client_capnp {
    add_gen!(%%/mozaic/client_capnp.rs%%);
}
