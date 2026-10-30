//! Operation security distinguishes inherited policy from an explicit override.
use salvo_core::Router;
use salvo_oapi::{OpenApi, Operation, PathItem, PathItemType, SecurityRequirement, endpoint};
use serde_json::{Value, json};

#[test]
fn operation_security_round_trip_preserves_inheritance_and_overrides() {
    let input = json!({
        "openapi": "3.1.0", "info": {"title": "Policies", "version": "1"},
        "security": [{"bearer": []}],
        "components": {"securitySchemes": {"bearer": {"type": "http", "scheme": "bearer"}}},
        "paths": {
            "/inherited": {"get": {"responses": {"200": {"description": "OK"}}}},
            "/public": {"get": {"responses": {"200": {"description": "OK"}}, "security": []}},
            "/optional": {"get": {"responses": {"200": {"description": "OK"}}, "security": [{}]}},
            "/secured": {"get": {"responses": {"200": {"description": "OK"}}, "security": [{"bearer": []}]}}
        }
    });
    let restored: OpenApi = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), input);
}

#[test]
fn empty_security_builder_removes_document_policy() {
    let mut doc = OpenApi::new("Policies", "1");
    doc.security.insert(SecurityRequirement::new(
        "bearer",
        std::iter::empty::<String>(),
    ));
    doc.paths.insert(
        "/public",
        PathItem::new(PathItemType::Get, Operation::new().securities([])),
    );
    doc.paths.insert(
        "/inherited",
        PathItem::new(PathItemType::Get, Operation::new()),
    );
    let value = serde_json::to_value(doc).unwrap();
    assert_eq!(
        value["paths"]["/public"]["get"].get("security"),
        Some(&json!([]))
    );
    assert!(
        value["paths"]["/inherited"]["get"]
            .get("security")
            .is_none()
    );
}

#[endpoint(security())]
async fn public_endpoint() {}
#[endpoint]
async fn inherited_endpoint() {}
#[endpoint(security(("bearer" = [])))]
async fn secured_endpoint() {}

#[test]
fn endpoint_macro_preserves_explicit_empty_security() {
    let router = Router::new()
        .push(Router::with_path("public").get(public_endpoint))
        .push(Router::with_path("inherited").get(inherited_endpoint))
        .push(Router::with_path("secured").get(secured_endpoint));
    let mut doc = OpenApi::new("Policies", "1").merge_router(&router);
    doc.security.insert(SecurityRequirement::new(
        "bearer",
        std::iter::empty::<String>(),
    ));
    let value: Value = serde_json::to_value(doc).unwrap();
    assert_eq!(
        value["paths"]["/public"]["get"].get("security"),
        Some(&json!([]))
    );
    assert!(
        value["paths"]["/inherited"]["get"]
            .get("security")
            .is_none()
    );
    assert_eq!(
        value["paths"]["/secured"]["get"]["security"],
        json!([{"bearer": []}])
    );
}

#[test]
fn route_security_and_added_requirements_remain_available() {
    use salvo_oapi::RouterExt;
    let policy = || SecurityRequirement::new("bearer", std::iter::empty::<String>());
    let router = Router::with_path("secured")
        .get(inherited_endpoint)
        .oapi_security(policy());
    let value = serde_json::to_value(OpenApi::new("Policies", "1").merge_router(&router)).unwrap();
    assert_eq!(
        value["paths"]["/secured"]["get"]["security"],
        json!([{"bearer": []}])
    );
    for operation in [
        Operation::new().add_security(policy()),
        Operation::new().securities([]).add_security(policy()),
    ] {
        assert_eq!(
            serde_json::to_value(operation).unwrap()["security"],
            json!([{"bearer": []}])
        );
    }
}
