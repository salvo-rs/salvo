//! Parameter registration does not introduce bindings that collide with endpoints.
use salvo_core::Router;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::{OpenApi, ToParameters, ToSchema, endpoint};
use serde_json::json;

mod plural {
    use super::*;

    #[derive(serde::Deserialize, ToParameters)]
    pub(super) struct Query<T: ToSchema = String> {
        value: T,
    }

    #[endpoint]
    pub(super) async fn parameters(query: Query<String>) -> String {
        query.value
    }
}

mod singular {
    use super::*;

    #[derive(serde::Deserialize, ToParameters)]
    pub(super) struct Query<T: ToSchema = String> {
        value: T,
    }

    #[endpoint]
    pub(super) async fn parameter(query: Query<String>) -> String {
        query.value
    }
}

#[tokio::test]
async fn endpoint_constructor_names_do_not_collide_with_parameter_registration() {
    for router in [
        Router::new().get(plural::parameters),
        Router::new().get(singular::parameter),
    ] {
        let document = json!(OpenApi::new("parameter names", "1").merge_router(&router));
        assert_eq!(
            document["paths"]["/"]["get"]["parameters"][0]["name"],
            "value"
        );
        let mut response = TestClient::get("http://localhost/?value=hello")
            .send(router)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "hello");
    }
}

#[derive(serde::Deserialize, ToParameters)]
struct Empty {}

#[test]
fn registering_empty_parameters_keeps_collection_types_inferable() {
    let mut operation = salvo_oapi::Operation::new();
    <Empty as salvo_oapi::EndpointArgRegister>::register(
        &mut salvo_oapi::Components::new(),
        &mut operation,
        "empty",
    );
    assert!(operation.parameters.is_empty());
}
