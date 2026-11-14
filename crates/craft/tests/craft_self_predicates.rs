//! Service predicates keep their original meaning in crafted handler wrappers.
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

trait Values {
    type Value;
}

#[derive(Clone, Debug)]
struct Service<T> {
    value: T,
}

impl<T> Values for Service<T> {
    type Value = T;
}

#[craft]
impl<T: Clone + Send + Sync + std::fmt::Display + 'static> Service<T>
where
    Self: Clone + Values,
    <Self as Values>::Value: std::fmt::Display,
{
    #[craft(handler)]
    fn show(&self) -> String {
        self.value.to_string()
    }
}

#[tokio::test]
async fn impl_self_and_associated_type_predicates_still_name_the_service() {
    let router = Router::new().get(Service { value: 7_u32 }.show());
    let body = TestClient::get("http://localhost/")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "7");
}
