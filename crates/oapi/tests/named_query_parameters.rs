//! Ordinary Query derives retain matching documentation and extraction behavior.

use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, ToParameters};

#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(default_parameter_in = Query))]
struct NamedQuery {
    filter: String,
    #[salvo(parameter(parameter_in = Query))]
    optional: Option<String>,
}

#[tokio::test]
async fn named_query_schema_matches_actual_named_extraction() {
    let parameters = NamedQuery::to_parameters(&mut Components::new());
    let value = serde_json::to_value(parameters).unwrap();
    assert_eq!(value[0]["in"], "query");
    assert_eq!(value[1]["in"], "query");
    assert!(value[0]["schema"].is_object());
    let mut req = TestClient::get("http://localhost/?filter=hello&optional=world").build();
    let query = NamedQuery::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(query.filter, "hello");
    assert_eq!(query.optional.as_deref(), Some("world"));
}
