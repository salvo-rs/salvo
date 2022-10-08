use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

use once_cell::sync::Lazy;
use salvo::prelude::*;
use salvo::Error;
use salvo_rate_limiter::{CelledQuota, MemoryStore, QuotaGetter, RateIssuer, RateLimiter, SlidingGuard};

static USER_QUOTAS: Lazy<HashMap<String, CelledQuota>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("user1".into(), CelledQuota::per_second(1, 1));
    map.insert("user2".into(), CelledQuota::set_seconds(1, 1, 5));
    map.insert("user3".into(), CelledQuota::set_seconds(1, 1, 10));
    map
});

struct UserIssuer;
#[async_trait]
impl RateIssuer for UserIssuer {
    type Key = String;
    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        req.query::<Self::Key>("user")
    }
}

struct CustomQuotaGetter;
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
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
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
            <a href="/limited?user=user1" target="_blank">Limited page for user1: 1/second</a>
        </p>
        <p>
            <a href="/limited?user=user2" target="_blank">Limited page for user2: 1/5seconds</a>
        </p>
        <p>
            <a href="/limited?user=user3" target="_blank">Limited page for user3: 1/10seconds</a>
        </p>
    </body>
</html>
"#;
