//! Parameter derives support borrowed fields and user lifetime predicates.
use std::marker::PhantomData;

use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, ToParameters};

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Borrowed<'a> {
    value: &'a str,
}

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Independent<'a, 'b, const N: usize>
where
    [u8; N]: AsRef<[u8]>,
{
    first: &'a str,
    second: &'b str,
    #[serde(skip)]
    marker: PhantomData<[u8; N]>,
}

#[tokio::test]
async fn single_borrowed_parameter_is_extracted_and_documented() {
    let mut req = TestClient::get("http://localhost/?value=hello").build();
    let mut depot = Depot::new();
    let value = Borrowed::extract(&mut req, &mut depot).await.unwrap();
    assert_eq!(value.value, "hello");
    let params = serde_json::json!(Borrowed::to_parameters(&mut Components::new()));
    assert_eq!(params[0]["name"], "value");
}

#[tokio::test]
async fn independent_parameter_lifetimes_keep_const_where_predicates() {
    let mut req = TestClient::get("http://localhost/?first=a&second=b").build();
    let mut depot = Depot::new();
    let value = Independent::<7>::extract(&mut req, &mut depot)
        .await
        .unwrap();
    assert_eq!((value.first, value.second), ("a", "b"));
}

#[derive(Debug, serde::Deserialize, ToParameters)]
struct SkippedLifetime<'a> {
    value: String,
    #[serde(skip)]
    marker: PhantomData<&'a str>,
}

#[tokio::test]
async fn skipped_parameter_lifetimes_do_not_constrain_the_request_borrow() {
    let mut req = TestClient::get("http://localhost/?value=hello").build();
    let mut depot = Depot::new();
    let value = SkippedLifetime::<'static>::extract(&mut req, &mut depot)
        .await
        .unwrap();
    assert_eq!(value.value, "hello");
}
