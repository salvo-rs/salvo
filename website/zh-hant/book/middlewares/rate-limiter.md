# Rate Limiter

提供流量控制功能的中間件.


## 主要功能

* `RateIssuer` 提供了對分配的識別訪問者身份鍵值的抽象. `RemoteIpIssuer` 是它的一個實現, 可以依據請求的 IP 地址確定訪問者. 鍵不一定是字符串類型, 任何滿足 `Hash + Eq + Send + Sync + 'static` 約束的類型都可以作為鍵.

* `RateGuard` 提供對流量控制算法的抽象. 默認實現了固定窗口(`FixedGuard`)和滑動窗口(`SlidingGuard`)兩個實現方式.

* `RateStore` 提供對數據的存取操作. `MemoryStore` 是內置的基於 `moka` 的一個內存的緩存實現. 你也可以定義自己的實現方式.

* `RateLimiter` 是實現了 `Handler` 的結構體, 內部還有一個 `skipper` 字段, 可以指定跳過某些不需要緩存的請求. 默認情況下, 會使用 `none_skipper` 不跳過任何請求.

* `QuotaGetter` 提供配額獲取的抽象, 可根據訪問者的 `Key` 獲取一個配額對象, 也就意味著我們可以把用戶配額等信息配置到數據庫中,動態改變, 動態獲取.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["rate-limiter"] }
```

## 簡單示例代碼

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
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
```


## 動態獲取配額示例代碼

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