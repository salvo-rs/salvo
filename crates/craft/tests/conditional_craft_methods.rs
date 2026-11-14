//! Disabled crafted methods do not report receiver errors in active builds.
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[derive(Clone)]
struct Service;
#[craft]
impl Service {
    #[cfg(any())]
    #[craft(handler)]
    fn disabled(&mut self) -> &'static str {
        "disabled"
    }

    #[cfg_attr(all(), cfg_attr(all(), cfg(any())), inline)]
    #[craft(endpoint)]
    fn nested_disabled(self: Box<Self>) -> &'static str {
        "disabled"
    }

    #[cfg_attr(all(), inline, cfg(all()))]
    #[craft(handler)]
    fn active(&self) -> &'static str {
        "active"
    }
}

struct Disabled;
#[craft]
impl Disabled {
    #[cfg(any())]
    #[craft(handler)]
    fn invalid(&mut self) {}
}

#[tokio::test]
async fn only_active_crafted_methods_are_checked_and_used() {
    let _disabled = Disabled;
    let response = TestClient::get("http://localhost/")
        .send(Router::new().get(Service.active()))
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(response, "active");
}
