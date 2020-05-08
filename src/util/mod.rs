pub mod request {
    use rand;
    use std::hash::Hash;

    #[derive(Clone, Debug, Copy, Hash, Eq, PartialEq, PartialOrd)]
    pub struct UUID(u64);
    impl UUID {
        pub fn new() -> Self {
            UUID(rand::random())
        }
    }
    impl std::convert::Into<u64> for UUID {
        fn into(self) -> u64 {
            self.0
        }
    }

    #[derive(Clone, Debug)]
    pub struct Req<T>(pub UUID, pub T);
    impl<T> Req<T> {
        pub fn new(t: T) -> Self {
            Req(UUID(rand::random()), t)
        }
        pub fn res<R>(&self, r: R) -> Res<R> {
            Res(self.0, r)
        }
    }
    impl<T: Default> Req<T> {
        pub fn default() -> Self {
            Req(UUID(rand::random()), T::default())
        }
    }

    #[derive(Clone, Debug)]
    pub struct Res<T>(pub UUID, pub T);
    impl<T> Res<T> {
        pub fn new<U: Into<UUID>>(uuid: U, t: T) -> Self {
            Res(uuid.into(), t)
        }
    }
    impl<T: Default> Res<T> {
        pub fn default<U: Into<UUID>>(uuid: U) -> Self {
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

    use crate::modules::types::PlayerId;

    #[derive(Clone, Debug)]
    pub enum Connect {
        Connected(PlayerId, String),    // TODO: make vec because of new cc
        Reconnecting(PlayerId, String),
        Waiting(PlayerId, u64),
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
