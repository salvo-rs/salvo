//! Empty examples lists compile for all schema builders that accept examples.
#![allow(dead_code)]

use salvo::oapi::{Components, RefOr, ToSchema};
use serde_json::{Value, json};

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let RefOr::Ref(reference) = T::to_schema(&mut components) else {
        panic!("expected a named schema");
    };
    json!({ "components": components })
        .pointer(&reference.ref_location[1..])
        .unwrap()
        .clone()
}

#[test]
fn empty_examples_on_structs_and_fields() {
    #[derive(ToSchema)]
    #[salvo(schema(examples()))]
    struct Payload {
        #[salvo(schema(examples()))]
        id: i32,
    }

    let schema = schema::<Payload>();
    assert!(schema.get("examples").is_none());
    assert!(schema["properties"]["id"].get("examples").is_none());
    assert_eq!(schema["properties"]["id"]["type"], "integer");
}

#[test]
fn empty_examples_on_newtypes_and_enums() {
    #[derive(ToSchema)]
    #[salvo(schema(examples()))]
    struct Newtype(String);

    #[derive(ToSchema)]
    #[salvo(schema(examples()))]
    enum State {
        Active,
        Disabled,
    }

    assert!(schema::<Newtype>().get("examples").is_none());
    let state = schema::<State>();
    assert!(state.get("examples").is_none());
    assert_eq!(state["enum"], json!(["Active", "Disabled"]));
}

#[test]
fn empty_examples_on_composed_schemas() {
    #[derive(serde::Deserialize, ToSchema)]
    struct Base {
        name: String,
    }

    #[derive(serde::Deserialize, ToSchema)]
    #[salvo(schema(examples()))]
    struct Extended {
        id: i32,
        #[serde(flatten)]
        base: Base,
    }

    let schema = schema::<Extended>();
    assert!(schema.get("allOf").is_some());
    assert!(schema.get("examples").is_none());
}

#[test]
fn nonempty_examples_remain_unchanged() {
    #[derive(ToSchema)]
    #[salvo(schema(examples(json!({ "id": 7 }))))]
    struct Payload {
        id: i32,
    }

    assert_eq!(schema::<Payload>()["examples"], json!([{ "id": 7 }]));
}
