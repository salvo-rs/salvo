// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
//port from https://github.com/seanmonstar/warp/blob/master/examples/sse.rs

use std::convert::Infallible;
use std::time::Duration;

use futures_util::StreamExt;
use salvo::prelude::*;
use salvo::sse::{self, SseEvent};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

// create server-sent event
fn sse_counter(counter: u64) -> Result<SseEvent, Infallible> {
    Ok(SseEvent::default().text(counter.to_string()))
}

#[handler]
async fn handle_tick(res: &mut Response) {
    let event_stream = {
        let mut counter: u64 = 0;
        let interval = interval(Duration::from_secs(1));
        let stream = IntervalStream::new(interval);
        stream.map(move |_| {
            counter += 1;
            sse_counter(counter)
        })
    };
    sse::streaming(res, event_stream).ok();
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("ticks").get(handle_tick);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
