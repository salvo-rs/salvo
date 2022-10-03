use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use once_cell::sync::Lazy;
use salvo::prelude::*;
use salvo::Error;
use salvo_rate_limiter::{MemoryStore, QuotaProvider, RateIssuer, RateLimiter, SimpleQuota, SlidingWindow};

static USER_QUOTAS: Lazy<HashMap<String, SimpleQuota>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("user1".into(), SimpleQuota::per_minute(1));
    map.insert("user2".into(), SimpleQuota::per_minute(10));
    map.insert("user3".into(), SimpleQuota::per_minute(60));
    map
});

pub struct UserIssuer;
#[async_trait]
impl RateIssuer for UserIssuer {
    type Key = String;
    async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key> {
        req.query::<Self::Key>("user")
    }
}

pub struct CustomQuotaProvider;
#[async_trait]
impl QuotaProvider<String> for CustomQuotaProvider {
    type Quota = SimpleQuota;
    type Error = Error;

    async fn get<Q>(&self, key: &Q) -> Result<Self::Quota, Self::Error>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        USER_QUOTAS.get(key).cloned().ok_or(Error::other("user not found"))
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
    let limiter = RateLimiter::new(
        SlidingWindow::new(),
        MemoryStore::new(),
        UserIssuer,
        CustomQuotaProvider,
    );
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
            This examples show how to set limit for different users. 
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
