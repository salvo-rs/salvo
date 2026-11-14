//! Untagged alternatives accept every valid serde representation, including overlaps.
#![allow(dead_code)]
use std::collections::BTreeMap;

use salvo::oapi::{Components, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum Numeric {
    Integer(i32),
    Number(f64),
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum Objects {
    Short { value: String },
    Long { value: String, extra: Option<u32> },
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum Nulls {
    First,
    Second,
    Number(u32),
}
#[derive(Serialize, Deserialize, ToSchema)]
enum ExternalFallback {
    Tagged(String),
    #[serde(untagged)]
    Fallback(BTreeMap<String, String>),
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum AdjacentFallback {
    Tagged(String),
    #[serde(untagged)]
    Fallback(BTreeMap<String, String>),
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum InternalFallback {
    Tagged {
        value: String,
    },
    #[serde(untagged)]
    Fallback(BTreeMap<String, String>),
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum UnitFallback {
    Tagged,
    #[serde(untagged)]
    Fallback,
}
#[derive(Serialize, Deserialize, ToSchema)]
enum ExternalUnitFallback {
    Tagged,
    #[serde(untagged)]
    Fallback,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum TaggedOnly {
    Tagged { value: String },
    Empty,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum AdjacentOnly {
    Tagged(String),
    Empty,
}
#[derive(Serialize, Deserialize, ToSchema)]
enum ExternalOnly {
    Tagged(String),
    Empty,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum SkippedFallback {
    Tagged {
        value: String,
    },
    #[serde(skip, untagged)]
    Fallback(BTreeMap<String, String>),
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum SchemaSkippedFallback {
    Tagged {
        value: String,
    },
    #[serde(untagged)]
    #[salvo(schema(skip))]
    Fallback(BTreeMap<String, String>),
}

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let reference = json!(T::to_schema(&mut components));
    let document = json!({"components": components});
    let mut schema = document
        .pointer(&reference["$ref"].as_str().unwrap()[1..])
        .unwrap()
        .clone();
    schema["components"] = document["components"].clone();
    schema
}
fn contract<T: ToSchema + Serialize>(values: Vec<T>) -> Value {
    json!({"schema": schema::<T>(), "values": values})
}

#[test]
fn untagged_schemas_allow_overlapping_values_and_variant_fallbacks() {
    let fallback = BTreeMap::from([("other".to_owned(), "yes".to_owned())]);
    let cases = json!({
        "numeric": contract(vec![Numeric::Integer(1), Numeric::Number(1.5)]),
        "objects": contract(vec![Objects::Short{value:"yes".into()}, Objects::Long{value:"yes".into(),extra:Some(2)}]),
        "nulls": contract(vec![Nulls::First, Nulls::Second, Nulls::Number(1)]),
        "external": contract(vec![ExternalFallback::Tagged("yes".into()),ExternalFallback::Fallback(fallback.clone())]),
        "adjacent": contract(vec![AdjacentFallback::Tagged("yes".into()),AdjacentFallback::Fallback(fallback.clone())]),
        "internal": contract(vec![InternalFallback::Tagged{value:"yes".into()},InternalFallback::Fallback(fallback)]),
        "unit": contract(vec![UnitFallback::Tagged,UnitFallback::Fallback]),
        "external_unit": contract(vec![ExternalUnitFallback::Tagged,ExternalUnitFallback::Fallback]),
    });
    for case in cases.as_object().unwrap().values() {
        assert!(case["schema"].get("anyOf").is_some());
        assert!(case["schema"].get("oneOf").is_none());
        assert!(case["schema"].get("discriminator").is_none());
    }
    assert_eq!(cases["unit"]["values"][1], Value::Null);
    assert_eq!(cases["unit"]["schema"]["anyOf"][1]["type"], "null");
    assert_eq!(cases["external_unit"]["values"][0], "Tagged");
    // The external validator uses this fixture to check actual serde payloads.
    println!("UNTAGGED_ENUM_CONTRACT={cases}");
}

#[test]
fn tagged_alternatives_and_excluded_fallbacks_keep_one_of() {
    for schema in [
        schema::<TaggedOnly>(),
        schema::<AdjacentOnly>(),
        schema::<SkippedFallback>(),
        schema::<SchemaSkippedFallback>(),
    ] {
        assert!(schema.get("oneOf").is_some());
        assert!(schema.get("anyOf").is_none());
        assert_eq!(schema["discriminator"]["propertyName"], "kind");
    }
    let external = schema::<ExternalOnly>();
    assert!(external.get("oneOf").is_some());
    assert!(external.get("anyOf").is_none());
}
