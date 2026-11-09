//! Deriving an extractor must preserve the type's original where predicates.

#![cfg(feature = "test")]

use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct QueryValue {
    value: String,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct WhereBound<T>
where
    T: Clone,
{
    #[salvo(extract(flatten))]
    inner: T,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct InlineBound<T: Clone> {
    #[salvo(extract(flatten))]
    inner: T,
}

#[tokio::test]
async fn inline_and_where_bounds_extract_the_same_request() {
    let mut req = TestClient::get("http://localhost/?value=expected").build();
    let mut depot = Depot::new();
    let value = WhereBound::<QueryValue>::extract(&mut req, &mut depot)
        .await
        .expect("where-bound extractor");
    assert_eq!(value.inner.value, "expected");

    let mut req = TestClient::get("http://localhost/?value=expected").build();
    let value = InlineBound::<QueryValue>::extract(&mut req, &mut depot)
        .await
        .expect("inline-bound extractor");
    assert_eq!(value.inner.value, "expected");
}

trait Named {
    type Name;
}

impl Named for QueryValue {
    type Name = String;
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct AssociatedBound<T>
where
    T: Named,
    T::Name: Clone,
{
    #[salvo(extract(flatten))]
    inner: T,
}

#[tokio::test]
async fn associated_type_where_predicates_are_preserved() {
    let mut req = TestClient::get("http://localhost/?value=associated").build();
    let value = AssociatedBound::<QueryValue>::extract(&mut req, &mut Depot::new())
        .await
        .expect("associated bound extractor");
    assert_eq!(value.inner.value, "associated");
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct MultipleBounds<T, U>
where
    T: Clone,
    U: Clone,
{
    #[salvo(extract(flatten))]
    first: T,
    #[salvo(extract(flatten))]
    second: U,
}

#[tokio::test]
async fn generated_predicates_for_multiple_types_form_a_valid_where_clause() {
    let mut req = TestClient::get("http://localhost/?value=both").build();
    let value = MultipleBounds::<QueryValue, QueryValue>::extract(&mut req, &mut Depot::new())
        .await
        .expect("multiple type parameters");
    assert_eq!(value.first.value, "both");
    assert_eq!(value.second.value, "both");
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct LifetimeBound<'a, 'b>
where
    'a: 'b,
{
    #[serde(borrow)]
    first: &'a str,
    #[serde(borrow)]
    second: &'b str,
}

#[tokio::test]
async fn lifetime_only_where_clause_keeps_borrowing_supported() {
    let mut req = TestClient::get("http://localhost/?first=one&second=two").build();
    let mut depot = Depot::new();
    let value: LifetimeBound<'_, '_> = LifetimeBound::extract(&mut req, &mut depot)
        .await
        .expect("borrowed extractor");
    assert_eq!((value.first, value.second), ("one", "two"));
}

trait SupportedSize {}
impl SupportedSize for [(); 2] {}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct ConstBound<const N: usize>
where
    [(); N]: SupportedSize,
{
    value: String,
}

#[tokio::test]
async fn const_generic_where_clause_without_type_parameters_is_preserved() {
    let mut req = TestClient::get("http://localhost/?value=constant").build();
    let value = ConstBound::<2>::extract(&mut req, &mut Depot::new())
        .await
        .expect("const-bound extractor");
    assert_eq!(value.value, "constant");
}
