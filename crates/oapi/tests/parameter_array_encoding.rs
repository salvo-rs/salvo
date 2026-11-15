//! Wire encodings advertised by ToParameters must work at request extraction.
use salvo_core::extract::Extractible;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Depot, Router, Service};
use salvo_oapi::{Components, ToParameters, endpoint};
use serde_json::json;

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Numbers {
    #[salvo(parameter(style = Form, explode = false))]
    values: Option<Vec<u32>>,
}
#[derive(Debug, serde::Deserialize, ToParameters)]
struct Borrowed<'a> {
    #[serde(borrow, alias = "q")]
    #[salvo(parameter(explode = false))]
    values: Vec<&'a str>,
}
#[derive(Debug, serde::Deserialize, ToParameters)]
#[salvo(parameters(default_parameter_in = Header, default_style = Simple))]
struct Headers {
    #[salvo(parameter(rename = "x-values"))]
    values: Vec<u32>,
    #[salvo(parameter(rename = "x-text"))]
    text: String,
}
#[derive(Debug, serde::Deserialize, ToParameters)]
struct Repeated {
    values: Vec<String>,
}
#[derive(Debug, serde::Deserialize, ToParameters)]
struct Pipes {
    #[salvo(parameter(style = PipeDelimited))]
    values: Vec<String>,
}

#[endpoint]
fn count(input: Numbers) -> String {
    input.values.unwrap_or_default().len().to_string()
}

#[tokio::test]
async fn query_encoding_matches_metadata_and_rejects_invalid_elements() {
    let params = json!(Numbers::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["style"], "form");
    assert_eq!(params[0]["explode"], false);
    assert_eq!(params[0]["schema"]["type"], "array");
    let service = Service::new(Router::new().get(count));
    let mut response = TestClient::get("http://localhost/?values=1,2,3")
        .send(&service)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "3");
    let response = TestClient::get("http://localhost/?values=1,no")
        .send(&service)
        .await;
    assert_eq!(response.status_code.unwrap().as_u16(), 400);
    let mut req = TestClient::get("http://localhost/").build();
    assert!(
        Numbers::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .values
            .is_none()
    );
}

#[tokio::test]
async fn escaped_delimiters_and_bracket_strings_stay_inside_borrowed_elements() {
    let mut req =
        TestClient::get("http://localhost/?%71=a%2Cb,c%2Bd,%E4%BD%A0%2C%E5%A5%BD,%5B1%2C2%5D,,")
            .build();
    let mut depot = Depot::new();
    let parsed = Borrowed::extract(&mut req, &mut depot).await.unwrap();
    assert_eq!(parsed.values, ["a,b", "c+d", "你,好", "[1,2]", "", ""]);
    let mut req = TestClient::get("http://localhost/?values=a%7Cb|c").build();
    assert_eq!(
        Pipes::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .values,
        ["a|b", "c"]
    );
}

#[tokio::test]
async fn header_arrays_split_and_trim_without_splitting_scalars() {
    let mut req = TestClient::get("http://localhost/")
        .add_header("x-values", "1, 2", false)
        .add_header("x-values", "3", false)
        .add_header("x-text", "a,b", true)
        .build();
    let parsed = Headers::extract(&mut req, &mut Depot::new()).await.unwrap();
    assert_eq!(parsed.values, [1, 2, 3]);
    assert_eq!(parsed.text, "a,b");
}

#[tokio::test]
async fn repeated_query_keys_and_middleware_changes_remain_available() {
    let mut req = TestClient::get("http://localhost/?values=a%2Cb&values=c").build();
    assert_eq!(
        Repeated::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .values,
        ["a,b", "c"]
    );
    let mut req = TestClient::get("http://localhost/?values=old").build();
    req.queries_mut().remove("values");
    req.queries_mut().insert("values".into(), "3,4".into());
    assert_eq!(
        Numbers::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .values,
        Some(vec![3, 4])
    );
}
