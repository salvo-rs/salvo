//! The documented `StatusError` schema describes the JSON body that is sent.

use salvo_core::Router;
use salvo_core::http::StatusError;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::{OpenApi, endpoint};
use serde_json::Value;

#[endpoint]
async fn plain() -> Result<String, StatusError> {
    Err(StatusError::bad_request().brief("not valid"))
}

#[endpoint]
async fn detailed() -> Result<String, StatusError> {
    Err(StatusError::bad_request()
        .brief("not valid")
        .detail("the detail")
        .cause("the cause"))
}

/// Checks `value` against an object schema: required names, known names and basic types.
fn assert_matches(schema: &Value, value: &Value, at: &str) {
    assert_eq!(schema["type"], "object", "{at}");
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{at} is not an object"));
    let properties = schema["properties"].as_object().unwrap();
    for name in schema["required"].as_array().into_iter().flatten() {
        let name = name.as_str().unwrap();
        assert!(object.contains_key(name), "{at} misses required `{name}`");
    }
    for (name, value) in object {
        let property = properties
            .get(name)
            .unwrap_or_else(|| panic!("{at} has undocumented `{name}`"));
        match property["type"].as_str() {
            Some("object") => assert_matches(property, value, &format!("{at}.{name}")),
            Some("string") => assert!(value.is_string(), "{at}.{name}"),
            Some("integer") => assert!(value.is_u64(), "{at}.{name}"),
            other => panic!("{at}.{name} has unexpected schema type {other:?}"),
        }
    }
}

#[tokio::test]
async fn json_error_body_matches_the_documented_schema() {
    let router = Router::new()
        .push(Router::with_path("plain").get(plain))
        .push(Router::with_path("detailed").get(detailed));
    let document =
        serde_json::to_value(OpenApi::new("test", "0.0.1").merge_router(&router)).unwrap();
    let reference = document["paths"]["/plain"]["get"]["responses"]["400"]["content"]
        ["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap();
    let name = reference.strip_prefix("#/components/schemas/").unwrap();
    let schema = &document["components"]["schemas"][name];

    let service = Service::new(router);
    for path in ["plain", "detailed"] {
        let mut response = TestClient::get(format!("http://localhost/{path}"))
            .add_header("accept", "application/json", true)
            .send(&service)
            .await;
        let body: Value = serde_json::from_str(&response.take_string().await.unwrap()).unwrap();
        assert_matches(schema, &body, path);
        assert_eq!(body["error"]["code"], 400);
        assert_eq!(body["error"]["brief"], "not valid");
    }
}
