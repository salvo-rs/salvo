//! Method predicates apply to the factory and its handler without restricting siblings.
use std::sync::Arc;

use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[derive(Clone)]
struct Service<T>(T);

#[craft]
impl<T: Send + Sync + 'static> Service<T> {
    #[craft(handler)]
    fn cloned(&self) -> &'static str
    where
        Self: Clone,
    {
        "cloned"
    }

    #[craft(handler)]
    fn displayed(self: Arc<Self>) -> String
    where
        T: std::fmt::Display,
    {
        self.0.to_string()
    }

    #[craft(handler)]
    fn stateless() -> &'static str
    where
        T: Clone,
    {
        "static"
    }

    #[craft(handler)]
    fn independent(self: Arc<Self>) -> &'static str {
        "independent"
    }
}

#[tokio::test]
async fn method_predicates_are_preserved_for_each_receiver() {
    struct NotClone;
    let service = Arc::new(Service(42_u32));
    let cases = [
        (Router::new().get(service.cloned()), "cloned"),
        (Router::new().get(service.displayed()), "42"),
        (Router::new().get(Service::<u32>::stateless()), "static"),
        (
            Router::new().get(Arc::new(Service(NotClone)).independent()),
            "independent",
        ),
    ];
    for (router, expected) in cases {
        let body = TestClient::get("http://localhost/")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, expected);
    }
}
