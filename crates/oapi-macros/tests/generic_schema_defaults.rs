//! Default schema values retain the concrete generic type being derived.
#![allow(dead_code)]

use salvo::oapi::{Components, ComposeSchema, RefOr, ToSchema};
use serde::Serialize;
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

#[derive(Default, Serialize, ToSchema)]
#[salvo(schema(default))]
struct Named<T: Default + Serialize + ToSchema + ComposeSchema> {
    value: T,
}

#[derive(Default, Serialize, ToSchema)]
#[salvo(schema(default))]
struct Newtype<T: Default + Serialize + ToSchema + ComposeSchema>(T);

#[derive(Default, Serialize, ToSchema)]
struct Base {
    active: bool,
}

#[derive(Default, Serialize, ToSchema)]
#[serde(default)]
struct Flattened<T: Default + Serialize + ToSchema + ComposeSchema> {
    value: T,
    #[serde(flatten)]
    base: Base,
}

#[test]
fn named_generic_defaults_follow_the_concrete_type() {
    assert_eq!(
        schema::<Named<String>>()["properties"]["value"]["default"],
        ""
    );
    assert_eq!(schema::<Named<i32>>()["properties"]["value"]["default"], 0);

    let mut components = Components::new();
    let generic = String::to_schema(&mut components);
    let composed = json!(Named::<String>::compose(&mut components, vec![generic]));
    assert_eq!(composed["properties"]["value"]["default"], "");
}

#[test]
fn generic_newtype_defaults_compile_and_compose() {
    assert_eq!(schema::<Newtype<String>>()["default"], "");
    let mut components = Components::new();
    let generic = String::to_schema(&mut components);
    let composed = json!(Newtype::<String>::compose(&mut components, vec![generic]));
    assert_eq!(composed["default"], "");
}

#[test]
fn flattened_generic_serde_defaults_use_the_concrete_self_type() {
    let expected = json!({ "value": "", "active": false });
    assert_eq!(schema::<Flattened<String>>()["default"], expected);
    let mut components = Components::new();
    let generic = String::to_schema(&mut components);
    let composed = json!(Flattened::<String>::compose(&mut components, vec![generic]));
    assert_eq!(composed["default"], expected);
}
