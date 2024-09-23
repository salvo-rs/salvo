# salvo-craft

[`Salvo`](https://github.com/salvo-rs/salvo) `Handler` modular craft macros.

[![Crates.io](https://img.shields.io/crates/v/salvo-craft)](https://crates.io/crates/salvo-craft)
[![Documentation](https://shields.io/docsrs/salvo-craft)](https://docs.rs/salvo-craft)

## `#[craft]`

`#[craft]` is an attribute macro used to batch convert methods in an `impl` block into [`Salvo`'s `Handler`](https://github.com/salvo-rs/salvo).

```rust
use salvo::oapi::extract::*;
use salvo::prelude::*;
use salvo_craft::craft;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let service = Arc::new(Service::new(1));
    let router = Router::new()
        .push(Router::with_path("add1").get(service.add1()))
        .push(Router::with_path("add2").get(service.add2()))
        .push(Router::with_path("add3").get(Service::add3()));
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[derive(Clone)]
pub struct Service {
    state: i64,
}

#[craft]
impl Service {
    fn new(state: i64) -> Self {
        Self { state }
    }
    /// doc line 1
    /// doc line 2
    #[craft(handler)]
    fn add1(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.state + *left + *right).to_string()
    }
    /// doc line 3
    /// doc line 4
    #[craft(handler)]
    pub(crate) fn add2(
        self: ::std::sync::Arc<Self>,
        left: QueryParam<i64>,
        right: QueryParam<i64>,
    ) -> String {
        (self.state + *left + *right).to_string()
    }
    /// doc line 5
    /// doc line 6
    #[craft(handler)]
    pub fn add3(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (*left + *right).to_string()
    }
}
```

Sure, you can also replace `#[craft(handler)]` with `#[craft(endpoint(...))]`.

NOTE: If the receiver of a method is `&self`, you need to implement the `Clone` trait for the type.
