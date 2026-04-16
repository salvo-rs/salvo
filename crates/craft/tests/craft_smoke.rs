#![allow(missing_docs)]

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
