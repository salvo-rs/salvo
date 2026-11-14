//! Request names follow serde's deserialization direction.
#![cfg(feature = "test")]
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;

#[derive(Debug, serde::Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
#[serde(rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "camelCase"))]
struct Input {
    first_value: String,
    #[serde(rename(serialize = "output", deserialize = "input"))]
    second_value: String,
}

#[tokio::test]
async fn field_and_container_names_use_the_input_direction() {
    let mut req = TestClient::get("http://localhost/?firstValue=first&input=second").build();
    let value = Input::extract(&mut req, &mut Depot::new()).await.unwrap();
    assert_eq!(
        (value.first_value, value.second_value),
        ("first".to_owned(), "second".to_owned())
    );
}
