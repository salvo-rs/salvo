# 使用模板引擎

Salvo 沒有內置任何模板引擎, 畢竟, 喜歡使用那種風格的模板引擎, 因人而異.

模板引擎本質上就是: 數據 + 模板 = 字符串.

所以, 只要能渲染最終的字符串就可以支持任意的模板引擎.

比如對 `askama` 的支持:

`templates/hello.html`:

```html
Hello, {{ name }}!
```

`src/main.rs`:

```rust
use askama::Template;
use salvo::prelude::*;

#[derive(Template)]
#[template(path = "hello.html")]
struct HelloTemplate<'a> {
    name: &'a str,
}

#[handler]
async fn hello_world(req: &mut Request, res: &mut Response) {
    let hello = HelloTemplate {
        name: req.param::<&str>("name").unwrap_or("World"),
    };
    res.render(Text::Html(hello.render().unwrap()));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let router = Router::with_path("<name>").get(hello_world);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
```