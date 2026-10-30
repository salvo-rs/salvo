//! OpenAPI 3.1 Schema Object references keep adjacent JSON Schema constraints.

use salvo_oapi::{OpenApi, Ref, RefOr, Schema};
use serde_json::json;

#[test]
fn schema_reference_keeps_adjacent_constraints() {
    let original = json!({
        "$ref": "#/components/schemas/Name",
        "description": "A short lowercase name",
        "minLength": 1,
        "maxLength": 8,
        "pattern": "^[a-z]+$",
        "not": {"enum": ["reserved"]},
        "x-source": {"system": "example"}
    });
    let parsed: RefOr<Schema> = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
}

#[test]
fn openapi_document_keeps_nested_reference_siblings() {
    let original = json!({
        "openapi": "3.1.0",
        "info": {"title": "Reference constraints", "version": "1"},
        "paths": {},
        "components": {"schemas": {
            "Name": {"type": "string"},
            "ShortName": {"$ref": "#/components/schemas/Name", "maxLength": 8},
            "Names": {"type": "array", "items": {
                "$ref": "#/components/schemas/Name", "minLength": 1
            }},
            "Record": {"type": "object", "properties": {"name": {
                "$ref": "#/components/schemas/Name", "default": "guest",
                "examples": ["alice"], "readOnly": true
            }}}
        }}
    });
    let document: OpenApi = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(document).unwrap(), original);
}

#[test]
fn ordinary_reference_serialization_keeps_its_existing_fields() {
    let reference = Ref::from_schema_name("Name")
        .summary("Name")
        .description("Shared name schema");
    assert_eq!(
        serde_json::to_value(reference).unwrap(),
        json!({"$ref": "#/components/schemas/Name", "summary": "Name", "description": "Shared name schema"})
    );
}

#[cfg(feature = "yaml")]
#[test]
fn reference_constraints_survive_yaml_round_trip() {
    let original = json!({
        "$ref": "#/components/schemas/Name",
        "maxLength": 8,
        "unevaluatedProperties": false,
        "x-source": {"system": "example"}
    });
    let yaml = serde_norway::to_string(&original).unwrap();
    let reference: RefOr<Schema> = serde_norway::from_str(&yaml).unwrap();
    assert_eq!(serde_json::to_value(&reference).unwrap(), original);
    let encoded = serde_norway::to_string(&reference).unwrap();
    let restored: RefOr<Schema> = serde_norway::from_str(&encoded).unwrap();
    assert_eq!(restored, reference);
}
