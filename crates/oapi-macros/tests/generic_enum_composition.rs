//! Enum representations preserve generic schema substitutions.
#![allow(dead_code)]
use salvo::oapi::{BasicType, Components, ComposeSchema, Object, RefOr, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(ToSchema)]
enum External<T> {
    Value(T),
    Fields { values: Vec<T> },
    Empty,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Internal<T> {
    Fields { value: T },
    Value(T),
    Empty,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum Adjacent<T> {
    Value(T),
    Fields { values: Vec<T> },
    Empty,
}
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
enum Untagged<T> {
    Value(T),
    Fields { value: Option<T> },
}

struct ComposeOnly;
impl ComposeSchema for ComposeOnly {
    fn compose(
        _: &mut Components,
        _: Vec<RefOr<salvo::oapi::schema::Schema>>,
    ) -> RefOr<salvo::oapi::schema::Schema> {
        Object::with_type(BasicType::Integer).into()
    }
}
fn composed<T: ComposeSchema>() -> Value {
    json!(T::compose(
        &mut Components::new(),
        vec![Object::with_type(BasicType::Integer).into()]
    ))
}

#[test]
fn all_enum_representations_use_supplied_generic_schemas() {
    let external = composed::<External<ComposeOnly>>();
    assert_eq!(
        external["oneOf"][0]["properties"]["Value"]["type"],
        "integer"
    );
    assert_eq!(
        external["oneOf"][1]["properties"]["Fields"]["properties"]["values"]["items"]["type"],
        "integer"
    );
    let internal = composed::<Internal<ComposeOnly>>();
    assert_eq!(
        internal["oneOf"][0]["properties"]["value"]["type"],
        "integer"
    );
    assert_eq!(internal["oneOf"][1]["allOf"][0]["type"], "integer");
    let adjacent = composed::<Adjacent<ComposeOnly>>();
    assert_eq!(
        adjacent["oneOf"][0]["properties"]["data"]["type"],
        "integer"
    );
    assert_eq!(
        adjacent["oneOf"][1]["properties"]["data"]["properties"]["values"]["items"]["type"],
        "integer"
    );
    let untagged = composed::<Untagged<ComposeOnly>>();
    let variants = untagged
        .get("anyOf")
        .or_else(|| untagged.get("oneOf"))
        .unwrap();
    assert_eq!(variants[0]["type"], "integer");
    assert_eq!(
        variants[1]["properties"]["value"]["oneOf"][1]["type"],
        "integer"
    );
}

#[test]
fn inferred_bounds_allow_concrete_enum_schema_generation() {
    let mut components = Components::new();
    for schema in [
        External::<String>::to_schema(&mut components),
        Internal::<ObjectPayload>::to_schema(&mut components),
        Adjacent::<String>::to_schema(&mut components),
        Untagged::<String>::to_schema(&mut components),
    ] {
        assert!(matches!(schema, RefOr::Ref(_)));
    }
}
#[derive(Serialize, ToSchema)]
struct ObjectPayload {
    id: u32,
}
