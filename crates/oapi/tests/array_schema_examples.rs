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
fn array_examples_apply_to_the_array() {
    #[derive(ToSchema)]
    struct Model {
        #[salvo(schema(examples(json!(["one", "two"]))))]
        values: Vec<String>,
    }
    let value = schema::<Model>();
    assert_eq!(
        value["properties"]["values"]["examples"],
        json!([["one", "two"]])
    );
    assert!(
        value["properties"]["values"]["items"]
            .get("examples")
            .is_none()
    );
}

#[test]
fn array_examples_support_empty_nested_and_referenced_items() {
    #[derive(ToSchema)]
    struct Item {
        id: u32,
    }
    #[derive(ToSchema)]
    struct Model {
        #[salvo(schema(examples()))]
        empty: Vec<String>,
        #[salvo(schema(examples(json!([[1, 2]]))))]
        nested: Vec<Vec<u32>>,
        #[salvo(schema(examples(json!([{"id": 1}]))))]
        referenced: Vec<Item>,
        #[salvo(schema(example = json!(["one"])))]
        singular: Vec<String>,
    }
    let value = schema::<Model>();
    assert!(value["properties"]["empty"].get("examples").is_none());
    assert_eq!(value["properties"]["nested"]["examples"], json!([[[1, 2]]]));
    assert!(
        value["properties"]["nested"]["items"]
            .get("examples")
            .is_none()
    );
    assert_eq!(
        value["properties"]["referenced"]["examples"],
        json!([[{"id": 1}]])
    );
    assert!(
        value["properties"]["referenced"]["items"]
            .get("$ref")
            .is_some()
    );
    assert_eq!(
        value["properties"]["singular"]["examples"],
        json!([["one"]])
    );
}
