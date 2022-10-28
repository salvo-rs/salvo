# Cache

Middleware that provides caching functionality.

Cache middleware can provide caching function for `StatusCode`, `Headers`, `Body` in `Response`. For the content that has been cached, when processing the request next time, Cache middleware will directly send the content cached in memory to the client.

Note that this plugin will not cache `Response` whose `Body` is a `ResBody::Stream`. If applied to a `Response` of this type, the Cache will not process these requests and will not cause error.

## Main Features

* `CacheIssuer` provides an abstraction over the assigned cache keys. `RequestIssuer` is an implementation of it that defines which parts of the requested URL and the requested `Method` to generate a cache key. You can also define your own The logic of cache key generation. The cache key does not have to be a string type, any type that satisfies the constraints of `Hash + Eq + Send + Sync + 'static` can be used as a key.
  
* `CacheStore` provides access to data. `MemoryStore` is a built-in `moka`-based memory cache implementation. You can also define your own implementation.
  
* `Cache` is a structure that implements `Handler`, and there is a `skipper` field inside, which can be specified to skip certain requests that do not need to be cached. By default, `MethodSkipper` will be used to skip all request except `Method::GET`.
  
  Internal implementation sample code:

  ```rust
  impl<S, I> Cache<S, I> {
    pub fn new(store: S, issuer: I) -> Self {
        let skipper = MethodSkipper::new().skip_all().skip_get(false);
        Cache {
            store,
            issuer,
            skipper: Box::new(skipper),
        }
    }
  }
  ```

## Config Cargo.toml

```toml
salvo = { version = "*", features = ["cache"] }
```

## Sample Code

```rust
use std::time::Duration;

use salvo::prelude::*;
use salvo::writer::Text;
use salvo::cache::{Cache, MemoryStore, RequestIssuer};
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
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
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
```