//! Conditional impl methods select both the signature and generated handler.
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Router, handler};

struct Conditional;
#[handler]
impl Conditional {
    #[cfg(all())]
    fn handle(&self) -> &'static str {
        "direct"
    }
    #[cfg(any())]
    fn handle(&self, _request: &mut salvo_core::Request) -> &'static str {
        "disabled"
    }
}

struct Nested;
#[handler]
impl Nested {
    #[cfg_attr(all(), inline, cfg(all()))]
    fn handle(&self) -> &'static str {
        "nested"
    }
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())), inline)]
    fn handle(&self, _invalid: &[u32]) -> &'static str {
        "disabled"
    }
}

struct Disabled;
#[handler]
impl Disabled {
    #[cfg(any())]
    fn handle(&self, _invalid: &[u32]) {}
}

#[tokio::test]
async fn rust_selects_the_configured_handler_method() {
    let _disabled = Disabled;
    for (router, expected) in [
        (Router::new().get(Conditional), "direct"),
        (Router::new().get(Nested), "nested"),
    ] {
        let response = TestClient::get("http://localhost/")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(response, expected);
    }
}
