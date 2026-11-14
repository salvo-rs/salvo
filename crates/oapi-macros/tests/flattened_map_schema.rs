//! Regression coverage for schemas containing flattened maps.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};

use salvo::oapi::{Components, RefOr, ToSchema};
use serde_json::json;

#[test]
fn flattened_map_with_primitive_values() {
    #[derive(serde::Deserialize, ToSchema)]
    struct Payload {
        id: i32,
        #[serde(flatten)]
        extra: BTreeMap<String, String>,
    }

    let mut components = Components::new();
    let RefOr::Ref(reference) = Payload::to_schema(&mut components) else {
        panic!("expected a named schema");
    };
    let document = json!({ "components": components });
    let schema = document.pointer(&reference.ref_location[1..]).unwrap();
    assert_eq!(schema["additionalProperties"], json!({ "type": "string" }));
    assert!(schema["properties"].get("id").is_some());
    assert!(schema["properties"].get("extra").is_none());
}

#[test]
fn flattened_map_registers_referenced_values() {
    #[derive(serde::Deserialize, ToSchema)]
    struct Detail {
        active: bool,
    }

    #[derive(serde::Deserialize, ToSchema)]
    struct Payload {
        id: i32,
        #[serde(flatten)]
        extra: HashMap<String, Detail>,
    }

    let mut components = Components::new();
    let RefOr::Ref(reference) = Payload::to_schema(&mut components) else {
        panic!("expected a named schema");
    };
    let document = json!({ "components": components });
    let schema = document.pointer(&reference.ref_location[1..]).unwrap();
    let detail_ref = schema["additionalProperties"]["$ref"].as_str().unwrap();
    let detail = document.pointer(&detail_ref[1..]).unwrap();
    assert_eq!(detail["properties"]["active"]["type"], "boolean");
    assert!(schema["properties"].get("extra").is_none());
}
