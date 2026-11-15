//! Public salvo/schema attributes apply to every enum variant parser.
#![allow(dead_code)]

use salvo::oapi::{Components, ToSchema};
use serde_json::{Value, json};

#[derive(serde::Serialize, ToSchema)]
enum Simple {
    #[salvo(schema(rename = "wire"))]
    Original,
    #[salvo(schema(skip))]
    Hidden,
}

#[derive(serde::Serialize, ToSchema)]
enum Mixed {
    Value(String),
    #[salvo(schema(title = "Readable", rename = "wire", example = "wire"))]
    Original,
}

#[derive(serde::Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Internal {
    Value {
        active: bool,
    },
    #[salvo(schema(title = "Readable", rename = "wire"))]
    Original,
}

#[derive(serde::Serialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum Adjacent {
    Value(String),
    #[salvo(schema(title = "Readable", rename = "wire"))]
    Original,
}

#[derive(serde::Serialize, ToSchema)]
#[serde(untagged)]
enum Untagged {
    Value(String),
    #[salvo(schema(title = "Readable"))]
    Empty,
}

#[derive(serde::Serialize, ToSchema)]
struct Detail {
    active: bool,
}

#[derive(serde::Serialize, ToSchema)]
enum InlineField {
    Value(#[salvo(schema(inline))] Detail),
}

struct Opaque;

#[derive(ToSchema)]
#[salvo(schema(value_type = Vec<String>))]
struct InlineOverride(#[salvo(schema(inline))] Opaque);

#[derive(ToSchema)]
enum InlineVariantOverride {
    #[salvo(schema(value_type = String))]
    Value(#[salvo(schema(inline))] Opaque),
}

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let value = json!(T::to_schema(&mut components));
    let document = json!({"components": components});
    match value.get("$ref").and_then(Value::as_str) {
        Some(reference) => document.pointer(&reference[1..]).unwrap().clone(),
        None => value,
    }
}

#[test]
fn simple_variant_rename_and_skip_use_public_attributes() {
    assert_eq!(schema::<Simple>()["enum"], json!(["wire"]));
}

#[test]
fn mixed_unit_variant_metadata_applies_to_every_representation() {
    let external = schema::<Mixed>();
    let variant = &external["oneOf"][1];
    assert_eq!(variant["enum"], json!(["wire"]));
    assert_eq!(variant["title"], "Readable");
    assert_eq!(variant["examples"], json!(["wire"]));
    for value in [schema::<Internal>(), schema::<Adjacent>()] {
        let variant = &value["oneOf"][1];
        assert_eq!(variant["title"], "Readable");
        assert_eq!(variant["properties"]["kind"]["enum"], json!(["wire"]));
    }
    assert_eq!(schema::<Untagged>()["anyOf"][1]["title"], "Readable");
}

#[test]
fn unnamed_variant_field_inline_removes_its_reference() {
    let value = schema::<InlineField>();
    let payload = &value["oneOf"][0]["properties"]["Value"];
    assert!(payload.get("$ref").is_none(), "{value}");
    assert_eq!(payload["properties"]["active"]["type"], "boolean");
}

#[test]
fn inline_newtypes_use_effective_value_types_without_original_schema_bounds() {
    let value = schema::<InlineOverride>();
    assert_eq!(value["type"], "array");
    assert_eq!(value["items"]["type"], "string");
    let value = schema::<InlineVariantOverride>();
    assert_eq!(value["oneOf"][0]["properties"]["Value"]["type"], "string");
}

#[cfg(feature = "repr")]
#[test]
fn repr_enum_container_metadata_uses_public_attributes() {
    #[derive(serde::Serialize, ToSchema)]
    #[repr(u8)]
    #[salvo(schema(title = "Numeric", name = "NumericReply", example = 7))]
    enum Numeric {
        Seven = 7,
    }
    let mut components = Components::new();
    let salvo::oapi::RefOr::Ref(reference) = Numeric::to_schema(&mut components) else {
        panic!("named enum");
    };
    assert!(reference.ref_location.ends_with("/NumericReply"));
    let document = json!({"components": components});
    let value = document.pointer(&reference.ref_location[1..]).unwrap();
    assert_eq!(value["title"], "Numeric");
    assert_eq!(value["examples"], json!([7]));
    assert_eq!(value["enum"], json!([7]));
}
