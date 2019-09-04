use std::io::{self, Write};

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


pub fn print_error(e: Error) {
    let mut stderr = io::stderr();

    for er in e.iter() {
        let _ = writeln!(stderr, "{}", er);
    }
}

pub struct ErrWrapper(Option<Error>);

impl<T> From<Result<T>> for ErrWrapper {
    fn from(e: Result<T>) -> Self {
        ErrWrapper(e.err())
    }
}

pub trait Consumable {
    fn consume(self);
}

impl<T> Consumable for T
    where T: Into<ErrWrapper> {

    fn consume(self) {
        self.into().consume();
    }
}

impl ErrWrapper {
    pub fn consume(self) {
        match self.0 {
            None => {},
            Some(inner) => print_error(inner),
        }
    }
}

pub use errors;
