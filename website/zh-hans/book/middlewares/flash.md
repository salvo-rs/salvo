# Flash

提供 Flash Message 的功能的中间件.

`FlashStore` 提供对数据的存取操作. `CookieStore` 会在 `Cookie` 中存储数据. 而 `SessionStore` 把数据存储在 `Session` 中, `SessionStore` 必须和 `session` 功能一起使用.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["flash"] }
```

## 示例代码

```rust
use std::fmt::Write;

use salvo::prelude::*;
use salvo::flash::{CookieStore, FlashDepotExt};

#[handler]
pub async fn set_flash(depot: &mut Depot, res: &mut Response) {
    let flash = depot.outgoing_flash_mut();
    flash.info("Hey there!").debug("How is it going?");
    res.render(Redirect::other("/get"));
}

#[handler]
pub async fn get_flash(depot: &mut Depot, _res: &mut Response) -> String {
    let mut body = String::new();
    if let Some(flash) = depot.incoming_flash() {
        for message in flash.iter() {
            writeln!(body, "{} - {}", message.value, message.level).unwrap();
        }
    }
    body
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let router = Router::new()
        .hoop(CookieStore::new().into_handler())
        .push(Router::with_path("get").get(get_flash))
        .push(Router::with_path("set").get(set_flash));
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```