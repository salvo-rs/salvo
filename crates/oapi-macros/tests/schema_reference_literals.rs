//! Endpoint schema references accept documented URI strings and retain typed references.
#![allow(dead_code)]

use salvo::Router;
use salvo::oapi::{OpenApi, ToSchema, endpoint};
use serde_json::json;

#[endpoint(request_body = ref("#/components/schemas/Payload"))]
async fn internal_request() {}

#[endpoint(request_body(
    content = ref("./external.json#/Payload"),
    content_type = "application/vnd.payload+json",
    description = "external payload"
))]
async fn external_request() {}

#[endpoint(responses((status_code = 200, body = ref("#/components/schemas/Payload"))))]
async fn internal_response() {}

#[endpoint(responses((
    status_code = 200,
    body = ref("https://example.com/schemas.json#/Payload"),
    content_type = "application/vnd.payload+json"
)))]
async fn external_response() {}

#[endpoint(responses((status_code = 200, content(
    ("application/json" = ref("./external.json#/Payload"))
))))]
async fn content_response() {}

#[derive(ToSchema)]
struct Payload {
    active: bool,
}

#[endpoint(request_body = ref(Payload), responses((status_code = 200, body = ref(Payload))))]
async fn typed_reference() {}

#[endpoint(request_body(content = ref(Payload), content_type = "application/xml"))]
async fn typed_content_type() {}

#[test]
fn uri_references_reach_request_and_response_documents() {
    let router = Router::new()
        .push(Router::with_path("internal-request").post(internal_request))
        .push(Router::with_path("external-request").post(external_request))
        .push(Router::with_path("internal-response").get(internal_response))
        .push(Router::with_path("external-response").get(external_response))
        .push(Router::with_path("content-response").get(content_response));
    let document = json!(OpenApi::new("references", "1").merge_router(&router));
    let paths = &document["paths"];
    assert_eq!(
        paths["/internal-request"]["post"]["requestBody"]["content"]["application/json"]["schema"],
        json!({ "$ref": "#/components/schemas/Payload" })
    );
    let external = &paths["/external-request"]["post"]["requestBody"];
    assert_eq!(external["description"], "external payload");
    assert_eq!(
        external["content"]["application/vnd.payload+json"]["schema"],
        json!({ "$ref": "./external.json#/Payload" })
    );
    assert_eq!(
        paths["/internal-response"]["get"]["responses"]["200"]["content"]["application/json"]["schema"],
        json!({ "$ref": "#/components/schemas/Payload" })
    );
    assert_eq!(
        paths["/external-response"]["get"]["responses"]["200"]["content"]["application/vnd.payload+json"]
            ["schema"],
        json!({ "$ref": "https://example.com/schemas.json#/Payload" })
    );
    assert_eq!(
        paths["/content-response"]["get"]["responses"]["200"]["content"]["application/json"]["schema"],
        json!({ "$ref": "./external.json#/Payload" })
    );
    assert!(document["components"]["schemas"].is_null());
}

#[test]
fn typed_references_still_register_their_schemas() {
    let router = Router::new()
        .push(Router::with_path("typed").post(typed_reference))
        .push(Router::with_path("typed-content-type").post(typed_content_type));
    let document = json!(OpenApi::new("typed references", "1").merge_router(&router));
    let operation = &document["paths"]["/typed"]["post"];
    let schema_ref = &operation["requestBody"]["content"]["application/json"]["schema"];
    assert_eq!(
        schema_ref,
        &operation["responses"]["200"]["content"]["application/json"]["schema"]
    );
    let reference = schema_ref["$ref"].as_str().unwrap();
    let schema = document.pointer(&reference[1..]).unwrap();
    assert_eq!(schema["properties"]["active"]["type"], "boolean");
    assert_eq!(
        &document["paths"]["/typed-content-type"]["post"]["requestBody"]["content"]["application/xml"]
            ["schema"],
        schema_ref
    );
}
