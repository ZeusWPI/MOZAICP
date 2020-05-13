pub mod request {
    use rand;
    use std::hash::Hash;

    #[derive(Clone, Debug, Copy, Hash, Eq, PartialEq, PartialOrd)]
    pub struct RequestID(u64);
    impl RequestID {
        pub fn new() -> Self {
            RequestID(rand::random())
        }
    }
    impl std::convert::Into<u64> for RequestID {
        fn into(self) -> u64 {
            self.0
        }
    }

    #[derive(Clone, Debug)]
    pub struct Req<T>(pub RequestID, pub T);
    impl<T> Req<T> {
        pub fn new(t: T) -> Self {
            Req(RequestID(rand::random()), t)
        }
        pub fn res<R>(&self, r: R) -> Res<R> {
            Res(self.0, r)
        }
    }
    impl<T: Default> Req<T> {
        pub fn default() -> Self {
            Req(RequestID(rand::random()), T::default())
        }
    }

    #[derive(Clone, Debug)]
    pub struct Res<T>(pub RequestID, pub T);
    impl<T> Res<T> {
        pub fn new<U: Into<RequestID>>(uuid: U, t: T) -> Self {
            Res(uuid.into(), t)
        }
    }
    impl<T: Default> Res<T> {
        pub fn default<U: Into<RequestID>>(uuid: U) -> Self {
            Res(uuid.into(), T::default())
        }
    }

    #[derive(Default, Clone, Debug)]
    pub struct Kill;
    #[derive(Clone, Debug)]
    pub enum State {
        Request,
        Response(Vec<Connect>),
    }
    impl State {
        pub fn res(&self) -> &Vec<Connect> {
            match self {
                State::Response(s) => s,
                _ => panic!("Tried getting state response, from a state request!"),
            }
        }
        pub fn req(self) -> () {
            match self {
                State::Request => (),
                _ => panic!("Tried getting state request, from a state response!"),
            }
        }
    }

    use crate::modules::types::{PlayerId, Uuid};

    #[derive(Clone, Debug)]
    pub enum Connect {
        Connected(PlayerId, String), // TODO: make vec because of new cc
        Reconnecting(PlayerId, String),
        Waiting(PlayerId, Uuid),
        Request,
    }

    use std::fmt;
    impl fmt::Display for Connect {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match self {
                Connect::Connected(_, name) => write!(f, "Connected {}", name),
                Connect::Reconnecting(_, name) => write!(f, "Reconnecting {}", name),
                Connect::Waiting(_, key) => write!(f, "Key to connect {}", key),
                Connect::Request => write!(f, "Request"),
            }
        }
    }
}

use crate::modules::types::Uuid;
use uuid::Builder;
pub fn gen_identification_key() -> Uuid {
    let random_bytes = rand::random();
    Builder::from_bytes(random_bytes).build()
}
