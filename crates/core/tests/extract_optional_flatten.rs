//! Optional nested extractors distinguish present, absent, and invalid data.
#![cfg(feature = "test")]
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Child<'a> {
    #[serde(alias = "legacy")]
    name: &'a str,
    count: u32,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Input<'a> {
    #[serde(borrow)]
    #[salvo(extract(flatten))]
    child: Option<Child<'a>>,
}

#[tokio::test]
async fn complete_input_produces_some_and_preserves_borrowing() {
    for query in ["name=hello&count=7", "legacy=hello&count=7"] {
        let mut req = TestClient::get(format!("http://localhost/?{query}")).build();
        let mut depot = Depot::new();
        let value = Input::extract(&mut req, &mut depot)
            .await
            .unwrap()
            .child
            .unwrap();
        assert_eq!((value.name, value.count), ("hello", 7));
    }
}

#[tokio::test]
async fn absent_input_produces_none_even_with_unrelated_values() {
    for query in ["", "unrelated=7"] {
        let mut req = TestClient::get(format!("http://localhost/?{query}")).build();
        assert!(
            Input::extract(&mut req, &mut Depot::new())
                .await
                .unwrap()
                .child
                .is_none()
        );
    }
}

#[tokio::test]
async fn partial_or_invalid_present_input_remains_an_error() {
    for query in ["name=hello", "count=7", "name=hello&count=bad"] {
        let mut req = TestClient::get(format!("http://localhost/?{query}")).build();
        assert!(Input::extract(&mut req, &mut Depot::new()).await.is_err());
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Layer<'a> {
    #[serde(borrow)]
    #[salvo(extract(flatten))]
    input: Input<'a>,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Root<'a> {
    #[serde(borrow)]
    #[salvo(extract(flatten))]
    layer: Option<Layer<'a>>,
}

#[tokio::test]
async fn nested_flatten_metadata_does_not_make_an_absent_child_present() {
    let mut req = TestClient::get("http://localhost/?unrelated=1").build();
    assert!(
        Root::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .layer
            .is_none()
    );
    let mut req = TestClient::get("http://localhost/?name=hello&count=7").build();
    assert_eq!(
        Root::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .layer
            .unwrap()
            .input
            .child
            .unwrap()
            .count,
        7
    );
}
