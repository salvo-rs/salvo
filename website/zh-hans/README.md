---
home: true
title: Home
heroImage: /images/logo-text.svg
heroText: null
actions:
  - text: 快速开始
    link: /zh-hans/book/guide.md
    type: primary
  - text: 资助项目
    link: /zh-hans/donate.md
    type: secondary
features:
  - title: 简单得让你一见钟情
    details: 你并不需要掌握非常复杂的 Rust 语言功能, 仅仅只需要里面的常见的功能, 就可以写出强大高效的服务器, 媲美 Go 类的 Web 服务器框架的开发速度.
  - title: 强大实用的功能
    details: 虽然简单, 但是功能依旧强大, 内置 Multipart, 灵活的数据解析...等等, 能满足大多数业务场景需求.
  - title: 风驰电掣的性能
    details: 在 Rust 的加持下, 性能报表. 与其他大多数语言的框架对比, 就像是他们拿着大炮, 你直接就出了核武器.
  - title: 从未见过的路由系统
    details: Salvo 拥有与众不同的路由系统, 可以无限嵌套, 使用方便, 灵活, 高效. 你可以用各种姿势随心所欲地使用它, 它能带给你前所未有的极致快感. 
  - title: 极简的中间件系统
    details: Salvo 中中间件和处理句柄都是 Handler, 两者合体, 和谐统一, 一片祥和. 官方提供丰富且灵活的中间件实现.
  - title: 运行稳定无忧
    details: Rust 极其安全的机制, 让你的网站上线后, 基本没有后顾之忧. 你有更多的时间和...在...啪啪啪享受性福时光, 而不是在电脑前焦头烂额地啪啪啪地敲着键盘抢救你的服务器程序.
footer: MIT Licensed | Copyright © 2019-present Salvo Team
---

### Hello world!

<CodeGroup>
  <CodeGroupItem title="main.rs" active>
  
```rust
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) {
    res.render("Hello world!");
}
#[tokio::main]
async fn main() {
    let router = Router::new().get(hello);
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
