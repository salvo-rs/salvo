//! Generic parameter derives require serde only when extracting values.
#![allow(dead_code)]
use std::marker::PhantomData;

use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, EndpointArgRegister, Operation, ToParameters, ToSchema};
use serde_json::json;

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Search<T: ToSchema = String> {
    value: T,
}

#[tokio::test]
async fn generic_owned_and_borrowed_values_extract_and_generate_parameters() {
    let mut req = TestClient::get("http://localhost/?value=hello").build();
    let value = Search::<String>::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(value.value, "hello");
    let mut req = TestClient::get("http://localhost/?value=borrowed").build();
    let mut depot = Depot::new();
    let value = Search::<&str>::extract(&mut req, &mut depot).await.unwrap();
    assert_eq!(value.value, "borrowed");
    assert_eq!(
        json!(Search::<String>::to_parameters(&mut Components::new()))[0]["name"],
        "value"
    );
}

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Mixed<'a, T: ToSchema = String, const N: usize = 3>
where
    [u8; N]: AsRef<[u8]>,
{
    value: T,
    borrowed: &'a str,
    #[serde(skip)]
    marker: PhantomData<[u8; N]>,
}

#[tokio::test]
async fn type_lifetime_and_const_generics_keep_user_predicates() {
    let mut req = TestClient::get("http://localhost/?value=7&borrowed=hello").build();
    let mut depot = Depot::new();
    let value = Mixed::<u32, 7>::extract(&mut req, &mut depot)
        .await
        .unwrap();
    assert_eq!((value.value, value.borrowed), (7, "hello"));
}

#[derive(ToSchema)]
struct DocumentationOnly {
    name: String,
}

#[test]
fn registering_parameter_documentation_does_not_require_deserialization() {
    let mut operation = Operation::new();
    Search::<DocumentationOnly>::register(&mut Components::new(), &mut operation, "search");
    assert_eq!(json!(operation.parameters)[0]["name"], "value");
}
