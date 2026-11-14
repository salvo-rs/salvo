//! Distinct concrete endpoint owner names keep distinct generated helpers.
#![allow(non_camel_case_types)]
use salvo_core::Router;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

struct 用户;
struct 订单;
mod nested {
    pub(super) struct Owner;
}
struct nested____Owner;

#[endpoint]
impl 用户 {
    fn handle(&self) -> &'static str {
        "user"
    }
}
#[endpoint]
impl 订单 {
    fn handle(&self) -> &'static str {
        "order"
    }
}
#[endpoint]
impl nested::Owner {
    fn handle(&self) -> &'static str {
        "nested"
    }
}
#[endpoint]
impl nested____Owner {
    fn handle(&self) -> &'static str {
        "flat"
    }
}

#[tokio::test]
async fn unicode_and_qualified_owner_helpers_do_not_collide() {
    let router = Router::new()
        .push(Router::with_path("user").get(用户))
        .push(Router::with_path("order").get(订单))
        .push(Router::with_path("nested").get(nested::Owner))
        .push(Router::with_path("flat").get(nested____Owner));
    let doc = json!(OpenApi::new("owners", "1").merge_router(&router));
    assert_eq!(doc["paths"].as_object().unwrap().len(), 4);
    let service = salvo_core::Service::new(router);
    for path in ["user", "order", "nested", "flat"] {
        assert_eq!(
            TestClient::get(format!("http://localhost/{path}"))
                .send(&service)
                .await
                .take_string()
                .await
                .unwrap(),
            path
        );
    }
}
