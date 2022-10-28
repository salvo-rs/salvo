# Use Template Engine

Salvo doesn't have any templating engine built in, after all, the style of templating you like to use varies from person to person.

A template engine is essentially: data + template = string.

So, any template engine can be supported as long as it can render the final string.

For example support for `askama`:

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