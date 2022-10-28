---
home: true
title: Home
heroImage: /images/logo-text.svg
heroText: null
actions:
  - text: Get Started
    link: /book/guid.md
    type: primary
  - text: Donate
    link: /donate.md
    type: secondary
features:
  - title: Simplicity First
    details: You just need the basic knowledge of Rust, you can write a powerful and efficient server, which is comparable to the development speed of some Go web server frameworks.
  - title: Powerful features
    details: Although it is simple, it is still powerful, with built-in Multipart, extract data from request, etc., which can meet the needs of most business scenarios.
  - title: Performance
    details: Thanks to the performance advantages of Rust, you can write extremely high-performance server-side applications very easily.
  - title: Chainable tree router
    details: Chainable tree routing system let you write routing rules easily and chains. You can use regex to constraint parameters.
  - title: Middlewares
    details: Flexible plugin API, allowing plugins to provide lots of plug-and-play features for your site. 
  - title: Stable after online
    details: Rust's extremely secure mechanism allows you to have no worries after your website is online. You have more time to enjoy your life!
footer: MIT Licensed | Copyright © 2019-present Salvo Team
---

### Hello world!

<CodeGroup>
  <CodeGroupItem title="main.rs" active>
  
```rust
use salvo::prelude::*;

#[handler]
async fn hello_world(res: &mut Response) {
    res.render("Hello world!");
}
#[tokio::main]
async fn main() {
    let router = Router::new().get(hello_world);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

  </CodeGroupItem>
  <CodeGroupItem title="Cargo.toml">
  
```toml
[package]
name = "example-hello"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
salvo = { version = "0.37" }
tokio = { version = "1", features = ["macros"] }
```

  </CodeGroupItem>
</CodeGroup>
