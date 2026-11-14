//! Serialization-only skips do not change the input contract.
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, ToParameters};

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Input {
    #[serde(skip_serializing)]
    token: String,
    #[serde(skip)]
    skipped: String,
    #[serde(skip_deserializing)]
    output: String,
}

#[tokio::test]
async fn input_only_fields_are_extracted_and_documented() {
    let mut req =
        TestClient::get("http://localhost/?token=secret&skipped=ignored&output=ignored").build();
    let input = Input::extract(&mut req, &mut Depot::new()).await.unwrap();
    assert_eq!(input.token, "secret");
    assert!(input.skipped.is_empty());
    assert!(input.output.is_empty());
    let params = serde_json::json!(Input::to_parameters(&mut Components::new()));
    assert_eq!(params.as_array().unwrap().len(), 1);
    assert_eq!(params[0]["name"], "token");
}
