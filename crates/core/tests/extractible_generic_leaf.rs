//! A query field uses Deserialize, not the Extractible trait of its value type.
#![allow(dead_code)]

use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;

#[derive(Debug, serde::Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Query<T> {
    value: T,
}

#[tokio::test]
async fn generic_leaf_field_accepts_string() {
    let mut req = TestClient::get("http://localhost/?value=hello").build();
    let value = Query::<String>::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(value.value, "hello");
}

#[derive(Debug, serde::Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Parent<T> {
    #[salvo(extract(flatten))]
    nested: Query<T>,
}

#[tokio::test]
async fn generic_children_keep_the_extractible_bound() {
    let mut req = TestClient::get("http://localhost/?value=42").build();
    let value = Parent::<u32>::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(value.nested.value, 42);
}
