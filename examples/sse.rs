// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
//port from https://github.com/seanmonstar/warp/blob/master/examples/sse.rs

use futures::StreamExt;
use salvo::prelude::*;
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

use salvo_extra::sse::{self, SseEvent};

// create server-sent event
fn sse_counter(counter: u64) -> Result<SseEvent, Infallible> {
    Ok(SseEvent::default().data(counter.to_string()))
}

#[fn_handler]
async fn handle_tick(_req: &mut Request, res: &mut Response) {
    let event_stream = {
        let mut counter: u64 = 0;
        let interval = interval(Duration::from_secs(1));
        let stream = IntervalStream::new(interval);
        let event_stream = stream.map(move |_| {
            counter += 1;
            sse_counter(counter)
        });
        event_stream
    };
    sse::streaming(res, event_stream);
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "sse=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let router = Router::new().path("ticks").get(handle_tick);
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
