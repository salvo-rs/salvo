//! Stateless crafted methods in generic impls keep their original scope.
use std::marker::PhantomData;

use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[derive(Debug)]
struct GenericService<T>(PhantomData<T>);

#[craft]
impl<T: Send + Sync + 'static> GenericService<T> {
    #[craft(handler)]
    fn ping() -> &'static str {
        "pong"
    }

    #[craft(endpoint)]
    fn documented() -> &'static str {
        "documented"
    }
    #[craft(endpoint)]
    fn increment(mut value: salvo::oapi::extract::QueryParam<i64>) -> String {
        *value += 1;
        value.to_string()
    }
}

struct ConstService<const N: usize>;
#[craft]
impl<const N: usize> ConstService<N> {
    #[craft(handler)]
    fn ping() -> &'static str {
        "const"
    }
}

#[allow(dead_code)]
struct BorrowedService<'a>(&'a str);
#[craft]
impl<'a> BorrowedService<'a> {
    #[craft(handler)]
    fn ping() -> &'static str {
        "borrowed"
    }
}

fn borrowed_handler<'a>(_: &'a str) -> impl Handler {
    BorrowedService::<'a>::ping()
}

#[tokio::test]
async fn static_methods_in_type_const_and_lifetime_generic_impls_run() {
    let owned = String::from("service");
    let routes = [
        (Router::new().get(GenericService::<u32>::ping()), "pong"),
        (Router::new().get(ConstService::<7>::ping()), "const"),
        (Router::new().get(borrowed_handler(&owned)), "borrowed"),
    ];
    for (router, expected) in routes {
        let body = TestClient::get("http://localhost/")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, expected);
    }
}

#[tokio::test]
async fn generic_static_endpoints_keep_runtime_and_documentation() {
    let router = Router::with_path("documented").get(GenericService::<u32>::documented());
    let doc = salvo::oapi::OpenApi::new("static", "1").merge_router(&router);
    assert!(doc.paths.get("/documented").is_some());
    let body = TestClient::get("http://localhost/documented")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "documented");
}

#[tokio::test]
async fn generic_static_endpoint_preserves_mutable_argument_bindings() {
    let router = Router::with_path("increment").get(GenericService::<u32>::increment());
    let doc = salvo::oapi::OpenApi::new("static", "1").merge_router(&router);
    let operation = doc
        .paths
        .get("/increment")
        .unwrap()
        .operations
        .get(&salvo::oapi::PathItemType::Get)
        .unwrap();
    let salvo::oapi::RefOr::Type(parameter) = &operation.parameters.0[0] else {
        panic!("expected parameter");
    };
    assert_eq!(parameter.name, "value");
    let body = TestClient::get("http://localhost/increment?value=2")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "3");
}
