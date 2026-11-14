//! Parameter names describe inputs while schemas describe serialization.
#![allow(dead_code)]
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, RefOr, ToParameters, ToSchema};
use serde_json::{Value, json};

#[derive(Debug, serde::Deserialize, serde::Serialize, ToParameters, ToSchema)]
#[serde(rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "camelCase"))]
struct Payload {
    first_value: String,
    #[serde(rename(serialize = "output", deserialize = "input"))]
    second_value: String,
}

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let RefOr::Ref(reference) = T::to_schema(&mut components) else {
        panic!("named schema expected")
    };
    json!({"components": components})
        .pointer(&reference.ref_location[1..])
        .unwrap()
        .clone()
}

#[tokio::test]
async fn parameters_use_input_names_and_schema_uses_output_names() {
    let mut req = TestClient::get("http://localhost/?firstValue=first&input=second").build();
    let value = Payload::extract(&mut req, &mut Depot::new()).await.unwrap();
    assert_eq!(
        json!(value),
        json!({"FIRST_VALUE":"first","output":"second"})
    );
    let params = json!(Payload::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["name"], "firstValue");
    assert_eq!(params[1]["name"], "input");
    let value = schema::<Payload>();
    assert!(value["properties"].get("FIRST_VALUE").is_some());
    assert!(value["properties"].get("output").is_some());
    assert!(value["properties"].get("input").is_none());
}

#[derive(Debug, serde::Deserialize, serde::Serialize, ToParameters, ToSchema)]
struct OneDirection {
    #[serde(rename(serialize = "output"))]
    input_default: String,
    #[serde(rename(deserialize = "input"))]
    output_default: String,
    #[serde(rename = "shared")]
    ordinary: String,
}

#[tokio::test]
async fn unspecified_directions_and_ordinary_names_keep_their_defaults() {
    let mut req = TestClient::get("http://localhost/?input_default=a&input=b&shared=c").build();
    let value = OneDirection::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(
        json!(value),
        json!({"output":"a","output_default":"b","shared":"c"})
    );
    let params = json!(OneDirection::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["name"], "input_default");
    assert_eq!(params[1]["name"], "input");
    assert_eq!(params[2]["name"], "shared");
    let value = schema::<OneDirection>();
    assert!(value["properties"].get("output").is_some());
    assert!(value["properties"].get("output_default").is_some());
    assert!(value["properties"].get("shared").is_some());
}
