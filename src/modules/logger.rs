use std::any;
use std::pin::Pin;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::prelude::*;

use crate::generic::*;

use std::fmt::Debug;

type BoxFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + 'a + Send>>;

#[derive(Clone, Debug)]
pub struct GameJoin(pub ReactorID);

pub trait LogHandler<T> {
    fn handle<'a>(&'a mut self, log: T) -> BoxFuture<'a>;
}

pub struct Logger<T> {
    tx: mpsc::UnboundedSender<T>,
    manager: ReactorID,
}

impl<T: 'static + Send + Clone> Logger<T> {
    pub fn params<H: LogHandler<T> + Send + 'static>(
        manager: ReactorID,
        handler: H,
        tp: ThreadPool,
    ) -> CoreParams<Self, any::TypeId, Message> {
        let (tx, rx) = mpsc::unbounded();
        tp.spawn_ok(start_handler(handler, rx));

        let me = Self { tx, manager };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_log))
            .handler(FunctionHandler::from(Self::handle_game_join))
    }

    fn handle_game_join(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        game: &GameJoin,
    ) {
        let link = LinkParams::new(()).external_handler(FunctionHandler::from(e_to_i::<(), T>(
            TargetReactor::Reactor,
        )));
        handle.open_link(game.0.clone(), link, false);
    }

    fn handle_log(&mut self, _handle: &mut ReactorHandle<any::TypeId, Message>, log: &T) {
        self.tx
            .unbounded_send(log.clone())
            .expect("Shit is failing here");
    }
}

impl<T> ReactorState<any::TypeId, Message> for Logger<T> {
    const NAME: &'static str = "Logger";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let manager_link =
            LinkParams::new(()).external_handler(FunctionHandler::from(e_to_i::<(), GameJoin>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.manager, manager_link, false);
    }
}

async fn start_handler<T, H: LogHandler<T> + Send + 'static>(
    mut handler: H,
    mut rx: mpsc::UnboundedReceiver<T>,
) {
    loop {
        if let Some(t) = rx.next().await {
            if let Err(e) = handler.handle(t).await {
                error!(%e);
            }
        } else {
            break;
        }
    }
}

pub use default::DefaultLogHandler;
mod default {
    use super::BoxFuture;
    use super::LogHandler;

    use async_std::fs::*;
    use async_std::path::Path;
    use async_std::prelude::*;
    use serde_json::Value;

    pub struct DefaultLogHandler {
        file: File,
    }

    impl DefaultLogHandler {
        pub async fn new<P: AsRef<Path>>(path: P) -> Option<Self> {
            let file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .await
                .ok()?;
            Some(DefaultLogHandler { file })
        }

        async fn write_line(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.file
                .write_all(bytes)
                .await
                .map_err(|e| format!("Logger error: {:?}", e))
        }
    }

    impl LogHandler<Value> for DefaultLogHandler {
        fn handle<'a>(&'a mut self, vs: Value) -> BoxFuture<'a> {
            Box::pin(async move {
                let mut bytes = serde_json::to_vec(&vs).unwrap();
                bytes.push(b'\n');
                self.write_line(&bytes).await?;
                self.file.flush().await.map_err(|_| "Cannot flush file!")?;
                Ok(())
            })
        }
    }
}
