//! Inline schemas retain arrays, tuples, unit types, and nested type arguments.
#![allow(dead_code)]
use salvo_core::Router;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::{Components, OpenApi, ToParameters, ToSchema, endpoint};
use serde_json::{Value, json};

#[endpoint(responses(
    (status_code = 200, body = inline([u8; 3]), headers(("x-values" = inline([u8; 2])))),
    (status_code = 201, body = inline((String, u32))),
    (status_code = 202, body = inline(()))
))]
fn response_shapes() {}

#[endpoint(request_body(content = inline([(String, u32); 2])))]
fn request_shapes() {}

#[endpoint(parameters(("values" = inline([u8; 3]), Query), ("pair" = inline((String, u32)), Query), ("unit" = inline(()), Query)))]
fn parameter_shapes() {}

#[derive(serde::Deserialize, ToParameters)]
struct Derived {
    #[salvo(parameter(inline))]
    fixed: [u8; 3],
    #[salvo(parameter(inline))]
    pair: (String, u32),
    #[salvo(parameter(inline))]
    unit: (),
}

fn assert_fixed(schema: &Value, count: usize) {
    assert_eq!(schema["type"], "array", "{schema}");
    assert_eq!(schema["minItems"], count);
    assert_eq!(schema["maxItems"], count);
}
fn assert_pair(schema: &Value) {
    assert_fixed(schema, 2);
    assert_eq!(schema["items"], false);
    assert_eq!(schema["prefixItems"][0]["type"], "string");
    assert_eq!(schema["prefixItems"][1]["type"], "integer");
}
fn response_schema(document: &Value, status: &str) -> Value {
    document["paths"]["/"]["get"]["responses"][status]["content"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()["schema"]
        .clone()
}

#[test]
fn response_and_header_inline_schemas_preserve_structural_types() {
    let document =
        json!(OpenApi::new("inline", "1").merge_router(&Router::new().get(response_shapes)));
    assert_fixed(&response_schema(&document, "200"), 3);
    assert_pair(&response_schema(&document, "201"));
    assert_eq!(response_schema(&document, "202")["type"], "null");
    assert_fixed(
        &document["paths"]["/"]["get"]["responses"]["200"]["headers"]["x-values"]["schema"],
        2,
    );
}

#[test]
fn request_body_inline_arrays_recursively_preserve_tuple_items() {
    let document =
        json!(OpenApi::new("inline", "1").merge_router(&Router::new().post(request_shapes)));
    let schema =
        &document["paths"]["/"]["post"]["requestBody"]["content"]["application/json"]["schema"];
    assert_fixed(schema, 2);
    assert_pair(&schema["items"]);
}

#[test]
fn explicit_and_derived_parameters_preserve_structural_types() {
    let document =
        json!(OpenApi::new("inline", "1").merge_router(&Router::new().get(parameter_shapes)));
    for values in [
        document["paths"]["/"]["get"]["parameters"].clone(),
        json!(Derived::to_parameters(&mut Components::new())),
    ] {
        assert_fixed(&values[0]["schema"], 3);
        assert_pair(&values[1]["schema"]);
        assert_eq!(values[2]["schema"]["type"], "null");
    }
}

#[derive(serde::Serialize, ToSchema)]
struct Child {
    value: String,
}
#[derive(serde::Serialize, ToSchema)]
struct Envelope<T> {
    child: T,
}
#[endpoint(responses((status_code = 200, body = inline(Envelope<[Child; 2]>))))]
fn generic_shapes() {}

#[test]
fn nested_generic_array_arguments_are_fully_inline() {
    let document =
        json!(OpenApi::new("inline", "1").merge_router(&Router::new().get(generic_shapes)));
    let schema = response_schema(&document, "200");
    assert_fixed(&schema["properties"]["child"], 2);
    assert_eq!(
        schema["properties"]["child"]["items"]["properties"]["value"]["type"],
        "string"
    );
    assert!(
        document
            .pointer("/components/schemas")
            .is_none_or(|schemas| schemas.as_object().unwrap().is_empty())
    );
    let serialized = json!(Envelope {
        child: [Child { value: "a".into() }, Child { value: "b".into() }]
    });
    assert_eq!(serialized["child"].as_array().unwrap().len(), 2);
}

#[endpoint(responses((status_code = 200, body = inline([u8; 3]), content_type = "application/json")))]
async fn array_payload() -> salvo_core::writing::Json<[u8; 3]> {
    salvo_core::writing::Json([1, 2, 3])
}

#[tokio::test]
async fn array_response_body_and_inline_document_describe_the_same_shape() {
    let router = Router::new().get(array_payload);
    let document = json!(OpenApi::new("inline", "1").merge_router(&router));
    assert_fixed(&response_schema(&document, "200"), 3);
    let mut response = TestClient::get("http://localhost/").send(router).await;
    assert_eq!(
        response.take_json::<Value>().await.unwrap(),
        json!([1, 2, 3])
    );
}
