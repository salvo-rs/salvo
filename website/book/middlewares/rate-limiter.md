# Rate Limiter

Middleware that provides flow control functionality.


## Main Features

* `RateIssuer` provides an abstraction of the assigned key value to identify the visitor's identity. `RemoteIpIssuer` is an implementation of it that can determine the visitor based on the requested IP address. Eq + Send + Sync + 'static` constraint types can be used as keys.

* `RateGuard` provides an abstraction for the flow control algorithm. By default, two implementations of fixed window (`FixedGuard`) and sliding window (`SlidingGuard`) are implemented.

* `RateStore` provides access to data. `MemoryStore` is a built-in `moka`-based memory cache implementation. You can also define your own implementation.

* `RateLimiter` is a structure that implements `Handler`, and there is also a `skipper` field inside, which can be specified to skip certain requests that do not require caching. By default, `none_skipper` will be used to not skip any requests.

* `QuotaGetter` provides the abstraction of quota acquisition, which can obtain a quota object according to the visitor's `Key`, which means that we can configure the user quota and other information into the database, change it dynamically, and acquire it dynamically.

## Config Cargo.toml

```toml
salvo = { version = "*", features = ["rate-limiter"] }
```

## Sample Code With Static Quota

``` rust
use salvo::prelude::*;
use salvo_rate_limiter::{BasicQuota, FixedGuard, MemoryStore, RateLimiter, RemoteIpIssuer};

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let limiter = RateLimiter::new(
        FixedGuard::new(),
        MemoryStore::new(),
        RemoteIpIssuer,
        BasicQuota::per_second(1),
    );
    let router = Router::with_hoop(limiter).get(hello_world);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```


## Sample Code With Dynnamic Quota

```rust
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

use once_cell::sync::Lazy;
use salvo::prelude::*;
use salvo::Error;
use salvo_rate_limiter::{CelledQuota, MemoryStore, QuotaGetter, RateIssuer, RateLimiter, SlidingGuard};

static USER_QUOTAS: Lazy<HashMap<String, CelledQuota>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("user1".into(), CelledQuota::per_minute(1, 1));
    map.insert("user2".into(), CelledQuota::per_minute(10, 5));
    map.insert("user3".into(), CelledQuota::per_minute(60, 10));
    map
});

pub struct UserIssuer;
#[async_trait]
impl RateIssuer for UserIssuer {
    type Key = String;
    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        req.query::<Self::Key>("user")
    }
}

pub struct CustomQuotaGetter;
#[async_trait]
impl QuotaGetter<String> for CustomQuotaGetter {
    type Quota = CelledQuota;
    type Error = Error;

    async fn get<Q>(&self, key: &Q) -> Result<Self::Quota, Self::Error>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        USER_QUOTAS
            .get(key)
            .cloned()
            .ok_or_else(|| Error::other("user not found"))
    }
}

#[handler]
async fn limited() -> &'static str {
    "Limited page"
}
#[handler]
async fn home() -> Text<&'static str> {
    Text::Html(HOME_HTML)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let limiter = RateLimiter::new(SlidingGuard::new(), MemoryStore::new(), UserIssuer, CustomQuotaGetter);
    let router = Router::new()
        .get(home)
        .push(Router::with_path("limited").hoop(limiter).get(limited));
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}

static HOME_HTML: &str = r#"
<!DOCTYPE html>
<html>
    <head>
        <title>Rate Limiter Dynmaic</title>
    </head>
    <body>
        <h2>Rate Limiter Dynamic</h2>
        <p>
            This example shows how to set limit for different users. 
        </p>
        <p>
            <a href="/limited?user=user1" target="_blank">Limited page for user1: 1/min</a>
        </p>
        <p>
            <a href="/limited?user=user2" target="_blank">Limited page for user2: 10/min</a>
        </p>
        <p>
            <a href="/limited?user=user3" target="_blank">Limited page for user3: 60/min</a>
        </p>
    </body>
</html>
"#;
```