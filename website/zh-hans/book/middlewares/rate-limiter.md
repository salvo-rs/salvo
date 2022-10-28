# Rate Limiter

提供流量控制功能的中间件.


## 主要功能

* `RateIssuer` 提供了对分配的识别访问者身份键值的抽象. `RemoteIpIssuer` 是它的一个实现, 可以依据请求的 IP 地址确定访问者. 键不一定是字符串类型, 任何满足 `Hash + Eq + Send + Sync + 'static` 约束的类型都可以作为键.

* `RateGuard` 提供对流量控制算法的抽象. 默认实现了固定窗口(`FixedGuard`)和滑动窗口(`SlidingGuard`)两个实现方式.

* `RateStore` 提供对数据的存取操作. `MemoryStore` 是内置的基于 `moka` 的一个内存的缓存实现. 你也可以定义自己的实现方式.

* `RateLimiter` 是实现了 `Handler` 的结构体, 内部还有一个 `skipper` 字段, 可以指定跳过某些不需要缓存的请求. 默认情况下, 会使用 `none_skipper` 不跳过任何请求.

* `QuotaGetter` 提供配额获取的抽象, 可根据访问者的 `Key` 获取一个配额对象, 也就意味着我们可以把用户配额等信息配置到数据库中,动态改变, 动态获取.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["rate-limiter"] }
```

## 简单示例代码

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


## 动态获取配额示例代码

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