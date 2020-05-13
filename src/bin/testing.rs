#[macro_use]
extern crate futures;
extern crate async_std;

use async_std::task::sleep;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use futures::task::{Context, Poll};
use futures::Future;

use std::pin::Pin;
use std::time::Duration;

fn time(sec: u64) -> Box<dyn Future<Output = ()> + Unpin> {
    let fut = async move { sleep(Duration::from_secs(sec)).await };
    Box::new(fut.boxed())
}

#[async_std::main]
async fn main() -> std::io::Result<()> {
    let mut thing = FuturesUnordered::new();
    thing.push(Fut(1, 0, time(5)));
    thing.push(Fut(2, 0, time(2)));
    thing.push(Fut(3, 0, time(1)));
    thing.push(Fut(4, 0, time(0)));

    loop {
        if poll!(thing.next()).is_ready() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
        println!("LOOPING");
    }

    Ok(())
}

struct Fut(usize, usize, Box<dyn Future<Output = ()> + Unpin>);

impl Future for Fut {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        println!("{}: {}", self.0, self.1);
        self.as_mut().1 += 1;

        let mut fut = self.as_mut();
        let pin = Pin::new(&mut fut.2);
        let _ = Future::poll(pin, context);

        Poll::Pending
    }
}
