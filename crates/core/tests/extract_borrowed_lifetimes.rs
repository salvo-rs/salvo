//! Extraction borrows may outlive every lifetime in the request type.
#![cfg(feature = "test")]
use std::marker::PhantomData;

use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;

#[derive(Debug, serde::Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Independent<'a, 'b> {
    first: &'a str,
    second: &'b str,
}

#[derive(Debug, serde::Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Constrained<'a, 'b, const N: usize>
where
    'a: 'b,
    [u8; N]: AsRef<[u8]>,
{
    first: &'a str,
    second: &'b str,
    #[serde(skip)]
    marker: PhantomData<[u8; N]>,
}

#[tokio::test]
async fn independent_borrows_are_extracted() {
    let mut req = TestClient::get("http://localhost/?first=a&second=b").build();
    let mut depot = Depot::new();
    let value = Independent::extract(&mut req, &mut depot).await.unwrap();
    assert_eq!((value.first, value.second), ("a", "b"));
}

#[tokio::test]
async fn borrowed_where_and_const_predicates_are_preserved() {
    let mut req = TestClient::get("http://localhost/?first=a&second=b").build();
    let mut depot = Depot::new();
    let value = Constrained::<7>::extract(&mut req, &mut depot)
        .await
        .unwrap();
    assert_eq!((value.first, value.second), ("a", "b"));
}

#[derive(Debug, serde::Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct SkippedLifetime<'a, 'b> {
    value: &'a str,
    #[serde(skip)]
    marker: PhantomData<&'b str>,
}

#[tokio::test]
async fn skipped_marker_lifetimes_do_not_constrain_the_request_borrow() {
    let mut req = TestClient::get("http://localhost/?value=hello").build();
    let mut depot = Depot::new();
    let value = SkippedLifetime::<'_, 'static>::extract(&mut req, &mut depot)
        .await
        .unwrap();
    assert_eq!(value.value, "hello");
}
