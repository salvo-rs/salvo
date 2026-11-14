//! Internal enum tags work with every flattened field schema shape.
#![allow(dead_code)]
use std::collections::BTreeMap;

use salvo::oapi::{Components, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

fn hide() -> bool {
    true
}
fn show() -> bool {
    false
}
#[derive(Serialize, ToSchema)]
struct Child {
    value: String,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Tagged {
    Plain {
        id: u32,
    },
    #[salvo(schema(title = "Mapped variant"))]
    Map {
        id: u32,
        #[serde(flatten)]
        extra: BTreeMap<String, String>,
    },
    Skipped {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(skip))]
        hidden: Child,
    },
    SerdeSkipped {
        id: u32,
        #[serde(flatten, skip)]
        hidden: Child,
    },
    Ignored {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(ignore))]
        hidden: Child,
    },
    ConditionalMap {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(ignore = hide))]
        extra: BTreeMap<String, String>,
    },
    ConditionalStruct {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(ignore = hide))]
        hidden: Child,
    },
    ShownMap {
        id: u32,
        #[serde(flatten)]
        #[salvo(schema(ignore = show))]
        extra: BTreeMap<String, String>,
    },
    Visible {
        #[serde(flatten)]
        child: Child,
    },
}

fn tag_object(schema: &Value) -> &Value {
    if schema.pointer("/properties/kind").is_some() {
        return schema;
    }
    schema["allOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item.pointer("/properties/kind").is_some())
        .unwrap()
}

#[test]
fn internally_tagged_fields_keep_their_tags_and_flattening_rules() {
    let mut components = Components::new();
    let reference = json!(Tagged::to_schema(&mut components));
    let document = json!({"components": components});
    let schema = document
        .pointer(&reference["$ref"].as_str().unwrap()[1..])
        .unwrap();
    let variants = schema["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), 9);
    for variant in variants {
        let object = tag_object(variant);
        assert!(
            object["required"]
                .as_array()
                .unwrap()
                .contains(&json!("kind"))
        );
        assert_eq!(
            object["properties"]["kind"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    assert!(variants[0].get("allOf").is_none());
    assert_eq!(
        tag_object(&variants[1])["additionalProperties"]["type"],
        "string"
    );
    assert_eq!(variants[1]["title"], "Mapped variant");
    for index in [2, 3, 4, 5] {
        assert!(variants[index].get("allOf").is_none());
        assert!(variants[index].get("additionalProperties").is_none());
        assert_eq!(variants[index]["properties"].as_object().unwrap().len(), 2);
    }
    assert_eq!(variants[6]["allOf"].as_array().unwrap().len(), 2);
    assert_eq!(variants[7]["additionalProperties"]["type"], "string");
    assert!(
        variants[8]["allOf"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.get("$ref").is_some())
    );
}
