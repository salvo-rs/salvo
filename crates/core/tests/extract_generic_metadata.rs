//! Generic flattened extractors must retain each child's fields and request sources.

#![cfg(feature = "test")]
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

use salvo_core::extract::Extractible;
use salvo_core::extract::metadata::SourceFrom;
use salvo_core::test::TestClient;
use salvo_core::{Depot, Request};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Query {
    value: String,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "header")))]
struct Header {
    value: String,
}

macro_rules! wrappers {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
        struct $name<T> {
            #[salvo(extract(flatten))]
            inner: T,
        }
    )+};
}

wrappers!(Forward, Reverse, Concurrent, Nested, Borrowing);

fn request() -> Request {
    TestClient::get("http://localhost/?value=query")
        .add_header("value", "header", true)
        .build()
}

#[tokio::test]
async fn generic_children_keep_their_sources_in_both_initialization_orders() {
    let query: Forward<Query> = request().extract(&mut Depot::new()).await.unwrap();
    let header: Forward<Header> = request().extract(&mut Depot::new()).await.unwrap();
    assert_eq!(query.inner.value, "query");
    assert_eq!(header.inner.value, "header");

    // A distinct wrapper gives the reverse order a fresh cache in this process.
    let header: Reverse<Header> = request().extract(&mut Depot::new()).await.unwrap();
    let query: Reverse<Query> = request().extract(&mut Depot::new()).await.unwrap();
    assert_eq!(header.inner.value, "header");
    assert_eq!(query.inner.value, "query");
}

#[tokio::test]
async fn concurrent_first_initialization_keeps_child_metadata_isolated() {
    let barrier = Arc::new(Barrier::new(16));
    let workers: Vec<_> = (0..16)
        .map(|index| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let (metadata, expected) = if index % 2 == 0 {
                    (
                        <Concurrent<Query> as Extractible>::metadata(),
                        SourceFrom::Query,
                    )
                } else {
                    (
                        <Concurrent<Header> as Extractible>::metadata(),
                        SourceFrom::Header,
                    )
                };
                assert_eq!(
                    metadata.fields[0].metadata.unwrap().default_sources[0].from,
                    expected
                );
                metadata
            })
        })
        .collect();
    let references: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    // One shared, stable allocation per composition even when initialization races.
    for (index, metadata) in references.iter().enumerate() {
        assert!(std::ptr::eq(*metadata, references[index % 2]));
    }
    assert!(!std::ptr::eq(references[0], references[1]));
    let query: Concurrent<Query> = request().extract(&mut Depot::new()).await.unwrap();
    let header: Concurrent<Header> = request().extract(&mut Depot::new()).await.unwrap();
    assert_eq!(query.inner.value, "query");
    assert_eq!(header.inner.value, "header");
}

#[tokio::test]
async fn recursively_composed_generic_metadata_does_not_lock_its_own_cache() {
    // Resolving the child re-enters Nested<T>::metadata before the parent can be cached.
    // A watchdog catches lock recursion without blocking the test runtime indefinitely.
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        sender
            .send(<Nested<Nested<Query>> as Extractible>::metadata())
            .unwrap();
    });
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("nested metadata initialization deadlocked");
    worker.join().unwrap();
    let query: Nested<Nested<Query>> = request().extract(&mut Depot::new()).await.unwrap();
    let header: Nested<Nested<Header>> = request().extract(&mut Depot::new()).await.unwrap();
    assert_eq!(query.inner.inner.value, "query");
    assert_eq!(header.inner.inner.value, "header");
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Borrowed<'a> {
    value: &'a str,
}

#[tokio::test]
async fn generic_children_can_borrow_from_the_request() {
    let mut req = request();
    let mut depot = Depot::new();
    let value: Borrowing<Borrowed<'_>> = req.extract(&mut depot).await.unwrap();
    assert_eq!(value.inner.value, "query");
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Pair<T> {
    #[salvo(extract(flatten))]
    left: Query,
    #[salvo(extract(flatten))]
    right: T,
}

#[tokio::test]
async fn each_flattened_child_contributes_to_the_metadata_key() {
    let first: Pair<Header> = request().extract(&mut Depot::new()).await.unwrap();
    let second: Pair<Query> = request().extract(&mut Depot::new()).await.unwrap();
    assert_eq!(
        (first.left.value.as_str(), first.right.value.as_str()),
        ("query", "header")
    );
    assert_eq!(
        (second.left.value.as_str(), second.right.value.as_str()),
        ("query", "query")
    );
}
