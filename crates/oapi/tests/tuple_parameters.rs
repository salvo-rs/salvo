//! Tuple parameters use the same field metadata as named request parameters.
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::ToParameters;

#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(names("id")))]
struct Id(u32);

#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(names("optional", "id", "token")))]
struct Mixed(
    Option<u32>,
    u32,
    #[salvo(parameter(parameter_in = Header, rename = "x-token"))] String,
);

#[tokio::test]
async fn newtype_query_parameter_is_extracted() {
    let mut req = TestClient::get("http://localhost/?id=42").build();
    assert_eq!(
        Id::extract(&mut req, &mut Depot::new()).await.unwrap().0,
        42
    );
}

#[tokio::test]
async fn tuple_slots_keep_their_sources_and_positions() {
    for query in ["id=42", "optional=7&id=42"] {
        let mut req = TestClient::get(format!("http://localhost/?{query}"))
            .add_header("x-token", "secret", true)
            .build();
        let data = Mixed::extract(&mut req, &mut Depot::new()).await.unwrap();
        assert_eq!(data.0, query.starts_with("optional").then_some(7));
        assert_eq!(data.1, 42);
        assert_eq!(data.2, "secret");
    }
}

#[tokio::test]
async fn missing_or_invalid_required_fields_return_errors() {
    for query in ["", "id=bad"] {
        let mut req = TestClient::get(format!("http://localhost/?{query}")).build();
        assert!(Id::extract(&mut req, &mut Depot::new()).await.is_err());
    }
}
