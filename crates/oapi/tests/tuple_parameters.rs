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

fn default_limit() -> Option<u32> {
    Some(25)
}

#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(names("id", "count", "limit")))]
struct Defaults(
    u32,
    #[serde(default)] u32,
    #[serde(default = "default_limit")] Option<u32>,
);

#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(names("id")))]
struct DefaultId(#[serde(default)] u32);

#[derive(Debug, Default, serde::Deserialize, ToParameters)]
#[serde(default)]
#[salvo(parameters(names("first", "second")))]
struct ContainerDefaults(u32, String);

#[tokio::test]
async fn absent_tuple_fields_invoke_serde_defaults() {
    let mut req = TestClient::get("http://localhost/?id=42").build();
    let value = Defaults::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!((value.0, value.1, value.2), (42, 0, Some(25)));
    let mut req = TestClient::get("http://localhost/?id=42&limit=9").build();
    let value = Defaults::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!((value.0, value.1, value.2), (42, 0, Some(9)));
    let mut req = TestClient::get("http://localhost/?id=42&count=7&limit=3").build();
    let value = Defaults::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!((value.0, value.1, value.2), (42, 7, Some(3)));
}

#[tokio::test]
async fn newtype_and_container_defaults_are_applied() {
    let mut req = TestClient::get("http://localhost/").build();
    assert_eq!(
        DefaultId::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .0,
        0
    );
    let mut req = TestClient::get("http://localhost/?second=value").build();
    let value = ContainerDefaults::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!((value.0, value.1), (0, "value".to_owned()));
}

#[tokio::test]
async fn invalid_values_do_not_fall_back_to_defaults() {
    for query in ["id=42&count=bad", "id=42&limit=bad"] {
        let mut req = TestClient::get(format!("http://localhost/?{query}")).build();
        assert!(
            Defaults::extract(&mut req, &mut Depot::new())
                .await
                .is_err()
        );
    }
}
