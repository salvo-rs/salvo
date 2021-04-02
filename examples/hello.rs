use salvo::prelude::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;
use salvo_core::routing::{ PathState};
use salvo_core::routing::filter::PathFilter;

#[tokio::main]
async fn main() {
    let filter = PathFilter::new("/hello/world<id>");
    let mut state = PathState::new("hello/worldabc");
    println!("{:?}", filter.detect(&mut state));
    println!("{:?}", state);
}
