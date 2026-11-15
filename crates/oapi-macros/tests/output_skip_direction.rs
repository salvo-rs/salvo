//! Output schema skips follow serde's serialization direction.
#![allow(dead_code)]
use salvo::oapi::{Components, RefOr, ToResponse, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema, ToResponse)]
enum Directional {
    Shared,
    #[serde(skip_deserializing)]
    OutputOnly,
    #[serde(skip_serializing)]
    InputOnly,
    #[serde(skip)]
    Neither,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct Fields {
    #[serde(skip_deserializing)]
    output: String,
    #[serde(skip_serializing)]
    input: String,
    #[serde(skip)]
    neither: String,
}

fn resolved<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    let document = json!({"components":components});
    match schema {
        RefOr::Ref(reference) => document
            .pointer(&reference.ref_location[1..])
            .unwrap()
            .clone(),
        RefOr::Type(schema) => json!(schema),
    }
}

#[test]
fn serialized_fields_and_variants_remain_in_output_schema() {
    assert_eq!(json!(Directional::OutputOnly), "OutputOnly");
    assert_eq!(
        resolved::<Directional>()["enum"],
        json!(["Shared", "OutputOnly"])
    );
    let fields = resolved::<Fields>();
    assert_eq!(fields["properties"]["output"]["type"], "string");
    assert_eq!(fields["required"], json!(["output"]));
    assert!(fields["properties"].get("input").is_none());
    assert!(fields["properties"].get("neither").is_none());
    let mut components = Components::new();
    let response = Directional::to_response(&mut components);
    let document = json!({"components":components});
    let response = match response {
        RefOr::Ref(reference) => document
            .pointer(&reference.ref_location[1..])
            .unwrap()
            .clone(),
        RefOr::Type(response) => json!(response),
    };
    assert_eq!(
        response["content"]["application/json"]["schema"]["enum"],
        json!(["Shared", "OutputOnly"])
    );
}
