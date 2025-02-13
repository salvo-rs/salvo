use std::time::Duration;

use salvo::cache::{Cache, MokaStore, RequestIssuer};
use salvo::prelude::*;
use salvo::writing::Text;
use time::OffsetDateTime;

// Handler for serving the home page with HTML content
#[handler]
async fn home() -> Text<&'static str> {
    Text::Html(HOME_HTML)
}

// Handler for short-lived cached response (5 seconds)
#[handler]
async fn short() -> String {
    format!(
        "Hello World, my birth time is {}",
        OffsetDateTime::now_utc()
    )
}

// Handler for long-lived cached response (1 minute)
#[handler]
async fn long() -> String {
    format!(
        "Hello World, my birth time is {}",
        OffsetDateTime::now_utc()
    )
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Create cache middleware for short-lived responses (5 seconds TTL)
    let short_cache = Cache::new(
        MokaStore::builder()
            .time_to_live(Duration::from_secs(5))
            .build(),
        RequestIssuer::default(),
    );

    // Create cache middleware for long-lived responses (60 seconds TTL)
    let long_cache = Cache::new(
        MokaStore::builder()
            .time_to_live(Duration::from_secs(60))
            .build(),
        RequestIssuer::default(),
    );

    // Set up router with three endpoints:
    // - / : Home page
    // - /short : Response cached for 5 seconds
    // - /long : Response cached for 1 minute
    let router = Router::new()
        .get(home)
        .push(Router::with_path("short").hoop(short_cache).get(short))
        .push(Router::with_path("long").hoop(long_cache).get(long));

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

// HTML template for the home page with links to cached endpoints
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
