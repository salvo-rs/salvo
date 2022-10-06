use std::time::Duration;

use salvo::cache::{Cache, MemoryStore, RequestIssuer};
use salvo::prelude::*;
use salvo::writer::Text;
use time::OffsetDateTime;

#[handler]
async fn home() -> Text<&'static str> {
    Text::Html(HOME_HTML)
}
#[handler]
async fn short() -> String {
    format!("Hello World, my birth time is {}", OffsetDateTime::now_utc())
}
#[handler]
async fn long() -> String {
    format!("Hello World, my birth time is {}", OffsetDateTime::now_utc())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let short_cache = Cache::new(
        MemoryStore::builder().time_to_live(Duration::from_secs(5)).build(),
        RequestIssuer::default(),
    );
    let long_cache = Cache::new(
        MemoryStore::builder().time_to_live(Duration::from_secs(60)).build(),
        RequestIssuer::default(),
    );
    let router = Router::new()
        .get(home)
        .push(Router::with_path("short").hoop(short_cache).get(short))
        .push(Router::with_path("long").hoop(long_cache).get(long));
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

static HOME_HTML: &str = r#"
<!DOCTYPE html>
<html>
    <head>
        <title>Cache Example</title>
    </head>
    <body>
        <h2>Cache Example</h2>
        <p>
            This examples shows how to use cache middleware. 
        </p>
        <p>
            <a href="/short" target="_blank">Cache 5 seconds</a>
        </p>
        <p>
            <a href="/long" target="_blank">Cache 1 minute</a>
        </p>
    </body>
</html>
"#;
