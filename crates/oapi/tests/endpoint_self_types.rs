//! Concrete impl endpoints can use Self in response type annotations.
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Response, Router, Scribe};
use salvo_oapi::{OpenApi, ToResponse, endpoint};

#[derive(ToResponse)]
struct Direct;
impl Scribe for Direct {
    fn render(self, res: &mut Response) {
        res.render("direct");
    }
}
#[endpoint]
impl Direct {
    fn handle(&self) -> Self {
        Self
    }
}

#[derive(ToResponse)]
struct Wrapped;
impl Scribe for Wrapped {
    fn render(self, res: &mut Response) {
        res.render("wrapped");
    }
}
#[endpoint]
impl Wrapped {
    fn handle(&self) -> Result<Self, salvo_core::http::StatusError> {
        Ok(Self)
    }
}

#[derive(ToResponse)]
struct Associated;
impl Scribe for Associated {
    fn render(self, res: &mut Response) {
        res.render("associated");
    }
}
trait Output {
    type Value;
}
impl Output for Associated {
    type Value = Self;
}
#[endpoint]
impl Associated {
    fn handle(&self) -> <Self as Output>::Value {
        Self
    }
}

#[tokio::test]
async fn self_response_types_work_in_metadata_and_requests() {
    for (handler, expected) in [
        (Router::with_path("value").get(Direct), "direct"),
        (Router::with_path("value").get(Wrapped), "wrapped"),
        (Router::with_path("value").get(Associated), "associated"),
    ] {
        let doc = serde_json::to_value(OpenApi::new("self", "1").merge_router(&handler)).unwrap();
        let reference = doc["paths"]["/value"]["get"]["responses"]["200"]["$ref"]
            .as_str()
            .unwrap();
        assert!(doc.pointer(&reference[1..]).is_some());
        let body = TestClient::get("http://localhost/value")
            .send(handler)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, expected);
    }
}
