//! Builtin data schemas accept the JSON values their writers serialize.
use std::marker::PhantomData;

use salvo_core::Router;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::writing::Json;
use salvo_oapi::{Components, ComposeSchema, OpenApi, ToSchema, endpoint};
use serde_json::{Value, json};

#[endpoint]
fn scalar() -> Json<Value> {
    Json(json!(7))
}
#[endpoint]
fn null() -> Json<Value> {
    Json(Value::Null)
}
#[endpoint]
fn array() -> Json<Value> {
    Json(json!([1, 2]))
}

#[test]
fn arbitrary_json_is_unconstrained_and_markers_are_null() {
    let mut components = Components::new();
    for schema in [
        Value::to_schema(&mut components),
        Value::compose(&mut components, vec![]),
    ] {
        // An empty schema accepts every JSON type, including null and arrays.
        assert_eq!(json!(schema), json!({}));
    }
    for schema in [
        PhantomData::<()>::to_schema(&mut components),
        PhantomData::<()>::compose(&mut components, vec![]),
    ] {
        assert_eq!(json!(PhantomData::<()>), Value::Null);
        assert_eq!(json!(schema), json!({"type":"null"}));
    }
    let raw = serde_json::value::RawValue::from_string("[1,true,null]".to_owned()).unwrap();
    assert_eq!(json!(raw), json!([1, true, null]));
    assert_eq!(
        json!(serde_json::value::RawValue::to_schema(&mut components)),
        json!({})
    );
}

#[tokio::test]
async fn json_value_endpoint_schemas_allow_actual_non_object_responses() {
    for (router, expected) in [
        (Router::new().get(scalar), json!(7)),
        (Router::new().get(null), Value::Null),
        (Router::new().get(array), json!([1, 2])),
    ] {
        let document = json!(OpenApi::new("values", "1").merge_router(&router));
        assert_eq!(
            document["paths"]["/"]["get"]["responses"]["200"]["content"]["application/json"]["schema"],
            json!({})
        );
        let body = TestClient::get("http://localhost/")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), expected);
    }
}
