//! Parameter extraction retains serde aliases in every field shape.
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, ToParameters};
use serde_json::json;

#[derive(Debug, serde::Deserialize, ToParameters)]
#[serde(rename_all = "camelCase")]
struct ContainerNames {
    #[serde(alias = "q", alias = "search")]
    filter_value: String,
}

#[derive(Debug, serde::Deserialize, ToParameters)]
#[serde(rename_all = "camelCase")]
struct Renamed {
    #[serde(rename = "serde_name", alias = "q", alias = "search")]
    #[salvo(parameter(rename = "request_name"))]
    filter_value: String,
}

#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(names("id", "filter")))]
struct Tuple(u32, #[serde(alias = "q", alias = "search")] String);

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Header {
    #[salvo(parameter(parameter_in = Header, rename = "x-filter"))]
    #[serde(alias = "x-query", alias = "x-search")]
    value: String,
}

#[tokio::test]
async fn aliases_work_with_container_case_rules() {
    for name in ["filterValue", "q", "search"] {
        let mut req = TestClient::get(format!("http://localhost/?{name}=hello")).build();
        let value = ContainerNames::extract(&mut req, &mut Depot::new())
            .await
            .unwrap();
        assert_eq!(value.filter_value, "hello");
    }
    let params = json!(ContainerNames::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["name"], "filterValue");
    assert_eq!(params.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn aliases_work_with_serde_and_parameter_rename_overrides() {
    for name in ["request_name", "q", "search"] {
        let mut req = TestClient::get(format!("http://localhost/?{name}=hello")).build();
        let value = Renamed::extract(&mut req, &mut Depot::new()).await.unwrap();
        assert_eq!(value.filter_value, "hello");
    }
    let params = json!(Renamed::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["name"], "request_name");
}

#[tokio::test]
async fn tuple_fields_accept_each_alias_without_shifting_slots() {
    for name in ["filter", "q", "search"] {
        let mut req = TestClient::get(format!("http://localhost/?id=42&{name}=hello")).build();
        let value = Tuple::extract(&mut req, &mut Depot::new()).await.unwrap();
        assert_eq!((value.0, value.1), (42, "hello".to_owned()));
    }
}

#[tokio::test]
async fn aliases_keep_the_field_header_source() {
    for name in ["x-filter", "x-query", "x-search"] {
        let mut req = TestClient::get("http://localhost/")
            .add_header(name, "hello", true)
            .build();
        let value = Header::extract(&mut req, &mut Depot::new()).await.unwrap();
        assert_eq!(value.value, "hello");
    }
}
