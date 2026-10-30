//! Regression coverage for mutually exclusive OpenAPI parameter representations.

use salvo_oapi::{Content, Object, OpenApi, Operation, Parameter, ParameterIn, Schema};
use serde_json::{Value, json};

fn document(parameter: Value, operation_level: bool) -> OpenApi {
    let mut path = json!({"get": {"responses": {"200": {"description": "OK"}}}});
    if operation_level {
        path["get"]["parameters"] = json!([parameter]);
    } else {
        path["parameters"] = json!([parameter]);
    }
    serde_json::from_value(json!({
        "openapi": "3.1.0",
        "info": {"title": "Parameters", "version": "1"},
        "paths": {"/items": path}
    }))
    .expect("valid input document")
}

fn schema_parameter() -> Value {
    json!({"name": "filter", "in": "query", "schema": {"type": "string"}})
}

fn content_parameter(media_type: &str) -> Value {
    json!({"name": "filter", "in": "query", "content": {
        media_type: {"schema": {"type": "object"}}
    }})
}

fn merged_parameter(first: Value, second: Value, operation_level: bool) -> Value {
    let merged = document(first, operation_level).merge(document(second, operation_level));
    let serialized = serde_json::to_value(merged).expect("serialize merged document");
    let parameters = if operation_level {
        &serialized["paths"]["/items"]["get"]["parameters"]
    } else {
        &serialized["paths"]["/items"]["parameters"]
    };
    assert_eq!(parameters.as_array().unwrap().len(), 1);
    parameters[0].clone()
}

#[test]
fn incoming_content_replaces_schema_in_merged_documents() {
    for operation_level in [false, true] {
        let incoming = content_parameter("application/json");
        let parameter = merged_parameter(schema_parameter(), incoming.clone(), operation_level);
        assert_eq!(parameter, incoming);
    }
}

#[test]
fn incoming_schema_replaces_content_in_merged_documents() {
    for operation_level in [false, true] {
        let incoming = schema_parameter();
        let parameter = merged_parameter(
            content_parameter("application/json"),
            incoming.clone(),
            operation_level,
        );
        assert_eq!(parameter, incoming);
    }
}

#[test]
fn incoming_content_replaces_the_entire_media_type_map() {
    for operation_level in [false, true] {
        let incoming = content_parameter("text/plain");
        let parameter = merged_parameter(
            content_parameter("application/json"),
            incoming.clone(),
            operation_level,
        );
        assert_eq!(parameter, incoming);
    }
}

#[test]
fn metadata_only_merge_preserves_the_existing_representation() {
    for mut original in [schema_parameter(), content_parameter("application/json")] {
        original["x-original"] = json!(true);
        original["x-shared"] = json!("before");
        let incoming = json!({
            "name": "filter", "in": "query", "description": "Updated metadata",
            "x-incoming": true, "x-shared": "after"
        });
        let merged = merged_parameter(original.clone(), incoming, false);
        original["description"] = json!("Updated metadata");
        original["x-incoming"] = json!(true);
        original["x-shared"] = json!("after");
        assert_eq!(merged, original);
    }
}

#[test]
fn representation_replacement_preserves_unrelated_parameter_fields() {
    let mut original = schema_parameter();
    original["description"] = json!("Keep this description");
    original["required"] = json!(true);
    original["x-original"] = json!(true);
    let mut incoming = content_parameter("application/json");
    incoming["x-incoming"] = json!(true);
    let merged = merged_parameter(original, incoming.clone(), false);
    incoming["description"] = json!("Keep this description");
    incoming["required"] = json!(true);
    incoming["x-original"] = json!(true);
    assert_eq!(merged, incoming);
}

#[test]
fn operation_parameter_insertion_uses_the_same_representation_precedence() {
    for (first, second) in [
        (schema_parameter(), content_parameter("application/json")),
        (content_parameter("application/json"), schema_parameter()),
        (
            content_parameter("application/json"),
            content_parameter("text/plain"),
        ),
    ] {
        let first: Parameter = serde_json::from_value(first).unwrap();
        let incoming: Parameter = serde_json::from_value(second.clone()).unwrap();
        let operation = Operation::new().parameters([first, incoming]);
        let value = serde_json::to_value(operation).unwrap();
        assert_eq!(value["parameters"], json!([second]));
    }
}

#[test]
fn parameter_builders_select_one_representation() {
    let schema = Schema::object(Object::new());
    let content = Content::new(schema.clone());
    let parameter = Parameter::new("filter")
        .location(ParameterIn::Query)
        .schema(schema.clone())
        .content("application/json", content.clone())
        .content("text/plain", content);
    let value = serde_json::to_value(&parameter).unwrap();
    assert!(value.get("schema").is_none());
    assert_eq!(value["content"].as_object().unwrap().len(), 1);
    assert!(value["content"].get("text/plain").is_some());

    let value = serde_json::to_value(parameter.schema(schema)).unwrap();
    assert!(value.get("content").is_none());
    assert!(value.get("schema").is_some());
}
