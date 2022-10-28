# 使用模板引擎

Salvo 没有内置任何模板引擎, 毕竟, 喜欢使用那种风格的模板引擎, 因人而异.

模板引擎本质上就是: 数据 + 模板 = 字符串.

所以, 只要能渲染最终的字符串就可以支持任意的模板引擎.

比如对 `askama` 的支持:

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
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```