//! Schema annotations describe the property and honor explicit false values.
#![allow(dead_code)]

use salvo_oapi::{Components, RefOr, ToSchema};
use serde_json::{Value, json};

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let RefOr::Ref(reference) = T::to_schema(&mut components) else {
        panic!("expected named schema");
    };
    json!({ "components": components })
        .pointer(&reference.ref_location[1..])
        .unwrap()
        .clone()
}

#[test]
fn skipped_flattened_fields_are_absent() {
    #[derive(serde::Deserialize, ToSchema)]
    struct Hidden {
        secret: String,
    }
    #[derive(serde::Deserialize, ToSchema)]
    struct Model {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(skip))]
        hidden: Hidden,
    }
    let value = schema::<Model>();
    assert!(value.get("allOf").is_none());
}

fn hidden() -> bool {
    true
}

#[test]
fn flattened_ignores_and_skips_do_not_leak_into_defaults() {
    #[derive(Default, serde::Serialize, ToSchema)]
    struct Child {
        secret: String,
    }
    #[derive(Default, serde::Serialize, ToSchema)]
    #[salvo(schema(default))]
    struct Model {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(skip))]
        skipped: Child,
        #[serde(flatten)]
        #[salvo(schema(ignore = hidden))]
        conditional: Child,
        #[serde(flatten)]
        #[salvo(schema(ignore))]
        ignored: Child,
        #[serde(flatten, skip)]
        serde_skipped: Child,
    }
    let value = schema::<Model>();
    assert_eq!(value["allOf"].as_array().unwrap().len(), 1);
    assert_eq!(value["default"], json!({"id": 0}));
}

#[test]
fn skipped_flattened_maps_do_not_conflict_with_visible_maps() {
    #[derive(serde::Serialize, ToSchema)]
    struct Model {
        #[serde(flatten)]
        #[salvo(schema(skip))]
        hidden: std::collections::BTreeMap<String, String>,
        #[serde(flatten)]
        visible: std::collections::BTreeMap<String, i32>,
    }
    let value = schema::<Model>();
    assert_eq!(value["additionalProperties"]["type"], "integer");
}
