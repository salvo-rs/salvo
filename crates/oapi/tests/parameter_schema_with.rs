//! Schema overrides and tuple parameters retain their extraction contract.
#![allow(dead_code)]

use salvo_oapi::{BasicType, Components, Object, ToParameters};
use serde_json::json;

fn custom_schema() -> Object {
    Object::new().schema_type(BasicType::String)
}

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Custom {
    /// Required search filter.
    #[salvo(parameter(schema_with = custom_schema, required = true, allow_reserved, explode = false, example = "demo"))]
    filter: String,
}

#[test]
fn schema_with_keeps_parameter_attributes() {
    let value = json!(Custom::to_parameters(&mut Components::new()));
    assert_eq!(value[0]["required"], true);
    assert_eq!(value[0]["allowReserved"], true);
    assert_eq!(value[0]["explode"], false);
    assert_eq!(value[0]["example"], "demo");
    assert_eq!(value[0]["description"], "Required search filter.");
}

#[derive(serde::Deserialize, ToParameters)]
struct Requiredness {
    #[salvo(parameter(schema_with = custom_schema))]
    required: String,
    #[salvo(parameter(schema_with = custom_schema))]
    optional: Option<String>,
    #[salvo(parameter(schema_with = custom_schema, required = false))]
    overridden: String,
    #[serde(default)]
    #[salvo(parameter(schema_with = custom_schema))]
    defaulted: String,
}

#[test]
fn schema_with_keeps_inferred_and_overridden_requiredness() {
    let value = json!(Requiredness::to_parameters(&mut Components::new()));
    for (index, expected) in [true, false, false, false].into_iter().enumerate() {
        assert_eq!(value[index]["required"], expected);
        assert_eq!(value[index]["schema"]["type"], "string");
    }
}
