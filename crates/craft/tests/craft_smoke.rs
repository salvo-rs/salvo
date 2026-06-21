#![allow(missing_docs, clippy::unwrap_used)]

extern crate salvo_core as salvo;

use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[derive(Clone, Debug)]
pub struct GreetingService {
    prefix: &'static str,
}

#[craft]
impl GreetingService {
    #[craft(handler)]
    fn hello(&self) -> String {
        format!("{} world", self.prefix)
    }
}

#[tokio::test]
async fn crafted_handler_can_be_registered_on_a_router() {
    let router = Router::new().get(GreetingService { prefix: "hello" }.hello());

    let body = TestClient::get("http://127.0.0.1:5801")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();

    assert_eq!(body, "hello world");
}

#[derive(Clone, Debug)]
pub struct Pair<A, B> {
    a: A,
    b: B,
}

// `impl<T> Pair<T, T>` is the regression case: the self type's arguments
// (`<T, T>`) differ from the impl's parameters (`<T>`), so generating the
// `handle` struct's impl headers from `self_ty` would emit `handle<T, T>` and
// fail to compile. `split_for_impl` yields the correct `handle<T>`.
#[craft]
impl<T: Clone + Send + Sync + std::fmt::Display + 'static> Pair<T, T> {
    #[craft(handler)]
    fn show(&self) -> String {
        format!("{}+{}", self.a, self.b)
    }
}

#[tokio::test]
async fn crafted_handler_supports_generic_impl_with_repeated_type_args() {
    let router = Router::new().get(Pair { a: "x", b: "y" }.show());

    let body = TestClient::get("http://127.0.0.1:5801")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();

    assert_eq!(body, "x+y");
}
