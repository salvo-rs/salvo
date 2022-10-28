# Affix

Affix 中间件用于往 Depot 中添加共享数据.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["affix"] }
```

## 示例代码

```rust
use salvo::affix;
use salvo::prelude::*;

#[handler]
async fn hello_world(depot: &mut Depot) -> String {
    let config = depot.obtain::<Config>().unwrap();
    let custom_data = depot.get::<&str>("custom_data").unwrap();
    format!("Hello World\nConfig: {:#?}\nCustom Data: {}", config, custom_data)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(route()).await;
}

#[allow(dead_code)]
#[derive(Default, Clone, Debug)]
struct Config {
    username: String,
    password: String,
}

fn route() -> Router {
    let config = Config {
        username: "root".to_string(),
        password: "pwd".to_string(),
    };
    Router::new()
        .hoop(affix::inject(config).insert("custom_data", "I love this world!"))
        .get(hello_world)
        .push(Router::with_path("hello").get(hello_world))
}
```