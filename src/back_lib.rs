#![allow(dead_code)]

extern crate bytes;
extern crate hex;

extern crate tokio;
#[macro_use]
extern crate futures;
extern crate rand;

extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate tracing;
extern crate tracing_futures;

#[macro_use]
extern crate error_chain;

extern crate capnp;
extern crate capnp_futures;

extern crate ws;

extern crate mozaic_derive;

// pub mod messaging;
// pub mod net;

// pub mod runtime;

pub mod modules;

// pub mod errors;

pub mod graph;
pub mod generic;

use capnp::traits::HasTypeId;

pub trait HasNamedTypeId: HasTypeId {
  fn get_name() -> &'static str {
    return "";
  }
}

///
/// Generate extra functions for this schema file.
/// Generates e_to_i external to internal, so retransmitting all internal messages of type to external
/// Generates i_to_e, idem dito but interal to external
///
// macro_rules! add_gen {
//   ($(
//     pub mod $name:ident {
//       $($content:tt)*
//     }
//   )*) => {
//     $(
//       pub mod $name {

//           pub fn e_to_i<C: crate::messaging::reactor::Ctx, T>(
//             _: &mut T,
//             h: &mut crate::messaging::reactor::LinkHandle<C>,
//             r: Reader) -> crate::errors::Result<()>
//         {
//             let m = crate::messaging::types::MsgBuffer::<Owned>::from_reader(r)?;
//             h.send_internal(m)?;
//             Ok(())
//         }

//         pub fn i_to_e<C: crate::messaging::reactor::Ctx, T>(
//             _: &mut T,
//             h: &mut crate::messaging::reactor::LinkHandle<C>,
//             r: Reader) -> crate::errors::Result<()>
//         {
//             let m = crate::messaging::types::MsgBuffer::<Owned>::from_reader(r)?;
//             h.send_message(m)?;
//             Ok(())
//         }

//         impl<'a> crate::HasNamedTypeId for Reader<'a> {
//             fn get_name()->&'static str {
//               stringify!($name)
//             }
//         }
//         impl<'a> crate::HasNamedTypeId for Builder<'a> {
//             fn get_name()->&'static str {
//               stringify!($name)
//             }
//         }

//         $($content)*
//       }
//     )*
//   };
// }

// /// Core capnp messages
// pub mod core_capnp {
//   add_gen!(%%/core_capnp.rs%%);
// }

// /// Network related capnp messages
// pub mod network_capnp {
//   add_gen!(%%/network_capnp.rs%%);
// }

// /// Generic MOZAIC messages
// pub mod base_capnp {
//   add_gen!(%%/mozaic/base_capnp.rs%%);
// }

// /// Messages specific for the steplock module
// pub mod steplock_capnp {
//   add_gen!(%%/mozaic/steplock_capnp.rs%%);
// }

// /// Messages for MOZAIC connection events
// pub mod connection_capnp {
//   add_gen!(%%/mozaic/connection_capnp.rs%%);
// }

// /// Messages specific for the cmd module
// pub mod cmd_capnp {
//   add_gen!(%%/mozaic/cmd_capnp.rs%%);
// }

// /// deprecated
// pub mod log_capnp {
//   add_gen!(%%/mozaic/logging_capnp.rs%%);
// }
