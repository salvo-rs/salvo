//! Endpoint metadata retains the owner's Self and nested item scopes.
use salvo_core::Router;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::writing::Json;
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

struct Service;
#[endpoint(
    operation_id = {
        trait LocalType {
            type Associated;
            const ID: &'static str;
            const NAME: &'static str = Self::ID;
        }
        struct Local;
        impl LocalType for Local {
            type Associated = Self;
            const ID: &'static str = "local";
        }
        let _: Option<<Local as LocalType>::Associated> = None::<Local>;
        assert_eq!(<Local as LocalType>::NAME, "local");
        Self::ID
    },
    status_codes(Self::CODE)
)]
impl Service {
    const WIDTH: usize = 3;
    const ID: &'static str = "service";
    const CODE: StatusCode = StatusCode::OK;

    fn handle(&self) -> Json<[u8; Self::WIDTH]> {
        Json([1; Self::WIDTH])
    }
}

#[tokio::test]
async fn self_constants_and_nested_item_bindings_work_in_metadata() {
    let router = Router::with_path("value").get(Service);
    let doc = json!(OpenApi::new("self scopes", "1").merge_router(&router));
    assert_eq!(doc["paths"]["/value"]["get"]["operationId"], "service");
    assert!(
        doc["paths"]["/value"]["get"]["responses"]
            .get("200")
            .is_some()
    );
    let body = TestClient::get("http://localhost/value")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "[1,1,1]");
}
