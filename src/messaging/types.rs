use std::marker::PhantomData;
use std::fmt;

use rand;
use rand::Rng;

use capnp;
use capnp::any_pointer;
use capnp::traits::{Owned, HasTypeId};
use core_capnp::mozaic_message;

use errors;

/// Handles messages of type M with lifetime 'a, using state S.
pub trait Handler<'a, S, M>: Send
    where M: Owned<'a>
{
    type Output;
    type Error;

    fn handle(&self, state: &mut S, reader: <M as Owned<'a>>::Reader)
        -> Result<Self::Output, Self::Error>;
}

/// Ties a handler function to a message type.
/// This coupling is required because a capnp reader type has a lifetime
/// parameter (for the buffer it refers to), and we want handlers to work
/// for any such lifetime parameter. This is achieved through the capnp
/// Owned<'a> trait, which M implements for all 'a. Through an associated
/// type on that trait, Reader<'a> is coupled.
/// Refer to the Handler implementation for FnHandler to see how this works.
pub struct FnHandler<M, F> {
    message_type: PhantomData<M>,
    function: F,
}

impl<M, F> FnHandler<M, F> {
    pub fn new(function: F) -> Self {
        FnHandler {
            function,
            message_type: PhantomData,
        }
    }

    pub fn typed(_m: M, function: F) -> Self {
        Self::new(function)
    }
}

impl<'a, S, M, F, T, E> Handler<'a, S, M> for FnHandler<M, F>
    where F: Fn(&mut S, <M as Owned<'a>>::Reader) -> Result<T, E>,
          F: Send,
          M: Owned<'a> + 'static + Send
{
    type Output = T;
    type Error = E;

    fn handle(&self, state: &mut S, reader: <M as Owned<'a>>::Reader)
        -> Result<T, E>
    {
        (self.function)(state, reader)
    }
}

/// Given a handler H for message type M, constructs a new handler that will
/// interpret an AnyPointer as M, and then pass it to H.
pub struct AnyPtrHandler<H, M> {
    message_type: PhantomData<M>,
    handler: H,
}

impl<H, M> AnyPtrHandler<H, M> {
    pub fn new(handler: H) -> Self {
        AnyPtrHandler {
            handler,
            message_type: PhantomData,
        }
    }
}

impl<'a, S, M, H> Handler<'a, S, any_pointer::Owned> for AnyPtrHandler<H, M>
    where H: Handler<'a, S, M>,
          H::Error: From<capnp::Error>,
          M: Send + Owned<'a>
{
    type Output = H::Output;
    type Error = H::Error;

    fn handle(&self, state: &mut S, reader: any_pointer::Reader<'a>)
        -> Result<H::Output, H::Error>
    {
        let m = reader.get_as()?;
        return self.handler.handle(state, m);
    }
}


pub type Ed25519Key = [u8; 32];

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct ReactorId {
    public_key: Ed25519Key,
}

impl fmt::Display for ReactorId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:x?}", self.public_key)
    }
}

impl fmt::Debug for ReactorId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:x?}", self.public_key)
    }
}

// TODO: this should be made secure
impl rand::distributions::Distribution<ReactorId> for rand::distributions::Standard {
    fn sample<G: Rng + ?Sized>(&self, rng: &mut G) -> ReactorId {
        ReactorId {
            public_key: rng.gen(),
        }
    }
}

impl ReactorId {
    pub fn bytes<'a>(&'a self) -> &'a [u8] {
        &self.public_key
    }
}

impl <'a> From<&'a [u8]> for ReactorId {
    fn from(src: &'a [u8]) -> ReactorId {
        assert!(src.len() == 32);

        let mut bytes = [0; 32];
        bytes.copy_from_slice(src);

        return ReactorId {
            public_key: bytes,
        };
    }
}

pub struct VecSegment {
    words: Vec<capnp::Word>,
}

impl VecSegment {
    pub fn new(words: Vec<capnp::Word>) -> Self {
        VecSegment {
            words,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() % 8 != 0 {
            panic!("invalid message");
        }
        let mut words = capnp::Word::allocate_zeroed_vec(bytes.len() / 8);
        capnp::Word::words_to_bytes_mut(&mut words[..])
            .copy_from_slice(bytes);
        return VecSegment { words };
    }

    pub fn as_bytes<'a>(&'a self) -> &'a [u8] {
        capnp::Word::words_to_bytes(&self.words)
    }
}

impl<'b> capnp::message::ReaderSegments for &'b VecSegment {
    fn get_segment<'a>(&'a self, idx: u32) -> Option<&'a [capnp::Word]> {
        if idx == 0 {
            return Some(&self.words);
        } else {
            return None;
        }
    }

    fn len(&self) -> usize {
        self.words.len()
    }
}

// TODO: it might be nice to make a message a reference-counted byte array,
// analogous to the Bytes type. On construction, it could be canonicalized
// and signed, then "frozen", just like Bytes. After that, it could easily be
// passed around the system.
pub struct Message {
    segment: VecSegment
}


impl Message {
    pub fn from_capnp<S>(reader: capnp::message::Reader<S>) -> Self
        where S: capnp::message::ReaderSegments
    {
        let words = reader.canonicalize().unwrap();
        let segment = VecSegment::new(words);
        return Message { segment };
    }

    pub fn from_segment(segment: VecSegment) -> Self {
        Message { segment }
    }

    pub fn reader<'a>(&'a self)
        -> capnp::message::TypedReader<&'a VecSegment, mozaic_message::Owned>
    {
        capnp::message::Reader::new(
            &self.segment,
            capnp::message::ReaderOptions::default(),
        ).into_typed()
    }

    pub fn bytes<'a>(&'a self) -> &'a [u8] {
        // todo: errors or something
        self.segment.as_bytes()
    }
}

use capnp::message::HeapAllocator;

pub struct MsgBuffer<T> {
    phantom_t: PhantomData<T>,
    builder: capnp::message::Builder<HeapAllocator>,
}

impl<T> MsgBuffer<T>
    where T: for<'a> Owned<'a>,
          <T as Owned<'static>>::Builder: HasTypeId,
{
    pub fn new() -> Self {
        let mut b = MsgBuffer {
            phantom_t: PhantomData,
            builder: capnp::message::Builder::new_default(),
        };

        b.init_builder();
        return b;
    }

    fn init_builder<'a>(&'a mut self) -> <T as Owned<'a>>::Builder {
        let mut msg: mozaic_message::Builder = self.builder.init_root();
        msg.set_type_id(<T as Owned<'static>>::Builder::type_id());
        return msg.init_payload().init_as();
    }

    fn get_builder<'a>(&'a mut self) -> <T as Owned<'a>>::Builder {
        let msg = self.builder.get_root::<mozaic_message::Builder>().unwrap();
        return msg.init_payload().get_as().unwrap();
    }

    pub fn build<'a, F>(&'a mut self, f: F)
        where F: FnOnce(&mut <T as Owned<'a>>::Builder)
    {
        let mut builder = self.get_builder();
        f(&mut builder);
    }

    pub fn from_reader<'a>(reader: <T as Owned<'a>>::Reader) -> errors::Result<Self> {

        let mut me = MsgBuffer::new();

        let msg = me.builder.get_root::<mozaic_message::Builder>().unwrap();
        msg.init_payload().set_as(reader).expect("Tha fuq");

        Ok(me)
    }
}

impl<T> MsgBuffer<T> {
    pub fn into_builder(self) -> capnp::message::Builder<HeapAllocator> {
        self.builder
    }
}
