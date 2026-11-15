//! Explicit inlining keeps parameter schema constraints.
#![allow(dead_code)]
use salvo_core::Router;
use salvo_oapi::{Components, OpenApi, ToParameters, ToSchema, endpoint};
use serde_json::json;

#[derive(serde::Deserialize, ToParameters)]
struct Parameters {
    #[salvo(parameter(inline, min_length = 3, nullable))]
    name: String,
    #[salvo(parameter(inline, min_items = 2, max_items = 4))]
    names: Vec<String>,
    #[salvo(parameter(inline))]
    optional: Option<String>,
}

#[endpoint(parameters(("name" = inline(String), Query, min_length = 3, nullable), ("names" = inline(Vec<String>), Query, min_items = 2, max_items = 4)))]
fn explicit() {}

#[test]
fn derived_inline_parameters_keep_constraints_and_optional_semantics() {
    let value = json!(Parameters::to_parameters(&mut Components::new()));
    assert_eq!(value[0]["schema"]["minLength"], 3);
    assert_eq!(value[0]["schema"]["type"], json!(["string", "null"]));
    assert_eq!(value[1]["schema"]["minItems"], 2);
    assert_eq!(value[1]["schema"]["maxItems"], 4);
    assert_eq!(value[2]["required"], false);
    assert_eq!(value[2]["schema"]["type"], "string");
}

#[test]
fn explicit_endpoint_inline_parameters_keep_constraints() {
    let value = json!(OpenApi::new("inline", "1").merge_router(&Router::new().get(explicit)));
    let parameters = &value["paths"]["/"]["get"]["parameters"];
    assert_eq!(parameters[0]["schema"]["minLength"], 3);
    assert_eq!(parameters[0]["schema"]["type"], json!(["string", "null"]));
    assert_eq!(parameters[1]["schema"]["minItems"], 2);
    assert_eq!(parameters[1]["schema"]["maxItems"], 4);
}

#[derive(serde::Deserialize, ToSchema)]
struct Child {
    name: String,
}
#[derive(serde::Deserialize, ToSchema)]
struct Envelope<T> {
    child: T,
}
#[derive(serde::Deserialize, ToParameters)]
struct Objects {
    #[salvo(parameter(inline, nullable))]
    object: Envelope<Child>,
    #[salvo(parameter(inline, min_items = 2))]
    objects: Vec<Envelope<Child>>,
}

#[test]
fn objects_and_generic_arguments_remain_fully_inline() {
    let mut components = Components::new();
    let value = json!(Objects::to_parameters(&mut components));
    let nullable = value[0]["schema"]
        .get("oneOf")
        .or_else(|| value[0]["schema"].get("anyOf"))
        .expect("nullable inline schema has a union")
        .as_array()
        .unwrap();
    assert!(
        nullable.iter().any(|schema| {
            schema["properties"]["child"]["properties"]["name"]["type"] == "string"
        })
    );
    assert!(components.schemas.is_empty());
    assert_eq!(value[1]["schema"]["minItems"], 2);
    assert_eq!(
        value[1]["schema"]["items"]["properties"]["child"]["properties"]["name"]["type"],
        "string"
    );
}

#[derive(serde::Deserialize, ToParameters)]
struct StructuralFeatures {
    #[salvo(parameter(inline, nullable))]
    fixed: [String; 3],
    #[salvo(parameter(inline, nullable))]
    pair: (String, u32),
    #[salvo(parameter(inline))]
    optional: Option<[u8; 3]>,
    #[salvo(parameter(inline))]
    generic: Envelope<[Child; 2]>,
}

#[test]
fn structural_type_syntax_and_parameter_features_are_preserved_together() {
    let mut components = Components::new();
    let value = json!(StructuralFeatures::to_parameters(&mut components));
    assert_eq!(value[0]["schema"]["type"], json!(["array", "null"]));
    assert_eq!(value[0]["schema"]["minItems"], 3);
    assert_eq!(value[0]["schema"]["maxItems"], 3);
    assert_eq!(value[1]["schema"]["type"], json!(["array", "null"]));
    assert_eq!(value[1]["schema"]["minItems"], 2);
    assert_eq!(value[1]["schema"]["prefixItems"][0]["type"], "string");
    assert_eq!(value[2]["required"], false);
    assert_eq!(value[2]["schema"]["type"], "array");
    assert_eq!(value[2]["schema"]["minItems"], 3);
    assert_eq!(value[3]["schema"]["properties"]["child"]["minItems"], 2);
    assert!(components.schemas.is_empty());
}
