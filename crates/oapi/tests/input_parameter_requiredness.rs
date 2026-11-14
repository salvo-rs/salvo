//! Serialization omissions do not relax a deserialization contract.
#![allow(dead_code)]
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, RefOr, ToParameters, ToSchema};
use serde_json::json;

#[derive(Debug, serde::Deserialize, serde::Serialize, ToParameters, ToSchema)]
struct Conditional {
    #[serde(skip_serializing_if = "String::is_empty")]
    value: String,
}

#[tokio::test]
async fn serialization_conditions_preserve_required_inputs_and_optional_outputs() {
    let params = json!(Conditional::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["required"], true);
    let mut req = TestClient::get("http://localhost/").build();
    assert!(
        Conditional::extract(&mut req, &mut Depot::new())
            .await
            .is_err()
    );
    let mut req = TestClient::get("http://localhost/?value=").build();
    let value = Conditional::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(json!(value), json!({}));
    let mut components = Components::new();
    let RefOr::Ref(reference) = Conditional::to_schema(&mut components) else {
        panic!("named schema expected")
    };
    let document = json!({"components": components});
    let schema = document.pointer(&reference.ref_location[1..]).unwrap();
    assert!(schema["properties"].get("value").is_some());
    assert!(schema.get("required").is_none());
}

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Shapes {
    #[serde(skip_serializing_if = "String::is_empty")]
    required: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    optional: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    defaulted: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    #[salvo(parameter(required = false))]
    explicitly_optional: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[salvo(parameter(required = true))]
    explicitly_required: Option<String>,
}

#[test]
fn option_defaults_and_explicit_parameter_overrides_keep_their_meaning() {
    let params = json!(Shapes::to_parameters(&mut Components::new()));
    for (index, expected) in [true, false, false, false, true].into_iter().enumerate() {
        assert_eq!(params[index]["required"], expected);
    }
}

#[derive(Debug, Default, serde::Deserialize, ToParameters)]
#[serde(default)]
struct DefaultContainer {
    #[serde(skip_serializing_if = "String::is_empty")]
    value: String,
}

#[tokio::test]
async fn container_defaults_still_allow_missing_parameters() {
    let params = json!(DefaultContainer::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["required"], false);
    let mut req = TestClient::get("http://localhost/").build();
    let value = DefaultContainer::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(value.value, "");
}
