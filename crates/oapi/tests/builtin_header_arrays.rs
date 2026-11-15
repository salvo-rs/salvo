//! Built-in header arrays decode Simple style without splitting scalar values.
use salvo_core::extract::Extractible;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Depot, Router, Service};
use salvo_oapi::extract::HeaderParam;
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

#[endpoint]
fn count(values: HeaderParam<Vec<u32>>) -> String {
    values.into_inner().len().to_string()
}

#[tokio::test]
async fn inferred_array_encoding_works_through_the_endpoint() {
    let router = Router::new().get(count);
    let document = json!(OpenApi::new("header arrays", "1").merge_router(&router));
    let parameter = &document["paths"]["/"]["get"]["parameters"][0];
    assert_eq!(parameter["in"], "header");
    assert_eq!(parameter["schema"]["type"], "array");
    assert!(parameter.get("style").is_none()); // Simple is the OpenAPI header default.
    let service = Service::new(router);
    let mut response = TestClient::get("http://localhost/")
        .add_header("values", "1,2,3", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.take_string().await.unwrap(), "3");
    let response = TestClient::get("http://localhost/")
        .add_header("values", "1,no,3", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
}

#[tokio::test]
async fn repeated_header_lines_preserve_element_order_and_borrowing() {
    let mut req = TestClient::get("http://localhost/")
        .add_header("values", "alpha, beta", false)
        .add_header("values", "gamma,delta", false)
        .build();
    let mut depot = Depot::new();
    let values = HeaderParam::<Vec<&str>>::extract_with_arg(&mut req, &mut depot, "values")
        .await
        .unwrap();
    assert_eq!(values.into_inner(), ["alpha", "beta", "gamma", "delta"]);
}

#[tokio::test]
async fn absent_header_arrays_respect_requiredness() {
    let mut req = TestClient::get("http://localhost/").build();
    let mut depot = Depot::new();
    assert!(
        HeaderParam::<Vec<u32>>::extract_with_arg(&mut req, &mut depot, "missing")
            .await
            .is_err()
    );
    assert!(
        HeaderParam::<Vec<u32>, false>::extract_with_arg(&mut req, &mut depot, "missing")
            .await
            .unwrap()
            .into_inner()
            .is_none()
    );
}

#[test]
fn sequences_split_after_options_and_newtypes_and_leave_scalars_unchanged() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct Numbers(Vec<u32>);
    let req = TestClient::get("http://localhost/?values=a,b")
        .add_header("values", "1, 2", false)
        .add_header("values", "3", false)
        .add_header("text", "alpha,beta", true)
        .build();
    assert_eq!(
        req.try_header::<Option<Numbers>>("values").unwrap(),
        Some(Numbers(vec![1, 2, 3]))
    );
    assert_eq!(req.try_header::<[u32; 3]>("values").unwrap(), [1, 2, 3]);
    assert_eq!(req.try_header::<String>("text").unwrap(), "alpha,beta");
    assert_eq!(
        req.try_header::<Option<&str>>("text").unwrap(),
        Some("alpha,beta")
    );
    assert_eq!(req.try_header::<Option<Vec<u32>>>("missing").unwrap(), None);
    assert_eq!(req.query::<Vec<String>>("values").unwrap(), ["a,b"]);
}
