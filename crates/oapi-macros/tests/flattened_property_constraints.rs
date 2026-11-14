//! Property-count constraints remain usable when a struct flattens another struct.
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
fn property_count_constraints_on_flattened_structs() {
    #[derive(serde::Deserialize, ToSchema)]
    struct Base {
        name: String,
    }

    #[derive(serde::Deserialize, ToSchema)]
    #[salvo(schema(max_properties = 5, min_properties = 2))]
    struct Extended {
        id: i32,
        #[serde(flatten)]
        base: Base,
    }

    let schema = schema::<Extended>();
    let all_of = schema["allOf"].as_array().unwrap();
    assert_eq!(all_of.len(), 2);
    assert!(all_of[0]["$ref"].is_string());
    assert_eq!(all_of[1]["maxProperties"], 5);
    assert_eq!(all_of[1]["minProperties"], 2);
    assert_eq!(all_of[1]["properties"]["id"]["type"], "integer");
}

#[test]
fn property_count_constraints_on_plain_structs() {
    #[derive(ToSchema)]
    #[salvo(schema(max_properties = 3, min_properties = 1))]
    struct Plain {
        id: i32,
    }

    let schema = schema::<Plain>();
    assert_eq!(schema["maxProperties"], 3);
    assert_eq!(schema["minProperties"], 1);
    assert!(schema.get("allOf").is_none());
}
