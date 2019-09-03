

use messaging::types::ReactorId;

// Create the Error, ErrorKind, ResultExt, and Result types
error_chain!{
    foreign_links {
        Capnp(::capnp::Error);
    }

    errors {
        NoSuchReactorError(id: ReactorId) {
            description("Reactor not found")
            display("No Reactor found with iod {:?}", id.bytes())
        }

        NoLinkFoundError(from: ReactorId, to: ReactorId) {
            description("Link not found")
            display("No Link from {:?} to {:?} found", from.bytes(), to.bytes())
        }

        MozaicError(msg: &'static str) {
            description("Generic MOZAIC Error")
            display("Generic MOZAIC error: {}", msg)
        }
    }
}
