//! Nullable composition must allow already-nullable child schemas.
#![allow(dead_code)]
use salvo_oapi::{Components, ComposeSchema, RefOr, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
struct Nullable<T> {
    value: Option<T>,
}

fn resolve(schema: RefOr<salvo_oapi::Schema>, components: Components) -> Value {
    let value = json!(schema);
    let document = json!({"components": components});
    let mut schema = value
        .get("$ref")
        .and_then(Value::as_str)
        .map(|reference| document.pointer(&reference[1..]).unwrap().clone())
        .unwrap_or(value);
    schema["components"] = document["components"].clone();
    schema
}

#[test]
fn nested_options_allow_null_in_native_composed_and_generic_schemas() {
    let mut components = Components::new();
    let native = <Option<Option<String>>>::to_schema(&mut components);
    let native = resolve(native, components);
    let mut components = Components::new();
    let composed = <Option<Option<String>>>::compose(&mut components, vec![]);
    let composed = resolve(composed, components);
    let mut components = Components::new();
    let generic = Nullable::<Option<String>>::to_schema(&mut components);
    let generic = resolve(generic, components);
    assert!(native.get("anyOf").is_some());
    assert!(composed.get("anyOf").is_some());
    assert!(generic["properties"]["value"].get("anyOf").is_some());
    let values = vec![None, Some(None), Some(Some("value".to_owned()))];
    println!(
        "NULLABLE_CONTRACT={}",
        json!([
            {"schema":native,"values":values,"invalid":[1,false,[]]},
            {"schema":composed,"values":values,"invalid":[1,false,[]]},
            {"schema":generic,"values":values.iter().cloned().map(|value|Nullable{value}).collect::<Vec<_>>(),"invalid":[{"value":1}]}
        ])
    );
}
