//! Generated context and extractor bindings cannot capture user identifiers.
#![allow(non_upper_case_globals)]
#![deny(non_local_definitions)]

use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Depot, FlowCtrl, Request, Response, Router, handler};
use salvo_oapi::extract::QueryParam;
use salvo_oapi::{OpenApi, endpoint};

#[handler]
fn handler_context(
    __macro_gen_req: QueryParam<String>,
    __macro_gen_depot: QueryParam<String>,
    __macro_gen_res: QueryParam<String>,
    __macro_gen_ctrl: QueryParam<String>,
    __macro_gen_conn: QueryParam<String>,
    __macro_gen_arg_0: QueryParam<String>,
) -> String {
    format!(
        "{__macro_gen_req}/{__macro_gen_depot}/{__macro_gen_res}/{__macro_gen_ctrl}/{__macro_gen_conn}/{__macro_gen_arg_0}"
    )
}

#[endpoint]
async fn endpoint_context(
    __macro_gen_req: QueryParam<u32>,
    __macro_gen_depot: QueryParam<u32>,
    __macro_gen_res: QueryParam<u32>,
    __macro_gen_ctrl: QueryParam<u32>,
    __macro_gen_conn: QueryParam<u32>,
    __macro_gen_arg_0: QueryParam<u32>,
    req: &Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    assert_eq!(req.uri().path(), "/endpoint");
    depot.insert("result", true);
    assert!(!ctrl.is_ceased());
    res.render(format!("{__macro_gen_req}/{__macro_gen_depot}/{__macro_gen_res}/{__macro_gen_ctrl}/{__macro_gen_conn}/{__macro_gen_arg_0}"));
}

mod request_named {
    use super::*;
    #[handler]
    pub(super) fn __macro_gen_req(__macro_gen_req: QueryParam<String>) -> String {
        __macro_gen_req.into_inner()
    }
}
mod extractor_named {
    use super::*;
    #[endpoint]
    pub(super) fn __macro_gen_arg_0(__macro_gen_arg_0: QueryParam<String>) -> String {
        __macro_gen_arg_0.into_inner()
    }
}

struct Generic<const __macro_gen_req: usize, const __macro_gen_depot: usize>;
#[handler]
impl<const __macro_gen_req: usize, const __macro_gen_depot: usize>
    Generic<{ __macro_gen_req }, { __macro_gen_depot }>
{
    fn handle(&self) -> String {
        format!("{__macro_gen_req}/{__macro_gen_depot}")
    }
}

#[tokio::test]
async fn internal_names_preserve_values_context_and_openapi_argument_names() {
    let router = Router::new()
        .push(Router::with_path("handler").get(handler_context))
        .push(Router::with_path("endpoint").get(endpoint_context))
        .push(Router::with_path("same-request").get(request_named::__macro_gen_req))
        .push(Router::with_path("same-extractor").get(extractor_named::__macro_gen_arg_0))
        .push(Router::with_path("generic").get(Generic::<7, 9>));
    let doc = serde_json::to_value(OpenApi::new("hygiene", "1").merge_router(&router)).unwrap();
    let names = doc["paths"]["/endpoint"]["get"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|parameter| parameter["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "macro_gen_req",
            "macro_gen_depot",
            "macro_gen_res",
            "macro_gen_ctrl",
            "macro_gen_conn",
            "macro_gen_arg_0"
        ]
    );
    let query = "macro_gen_req=1&macro_gen_depot=2&macro_gen_res=3&macro_gen_ctrl=4&macro_gen_conn=5&macro_gen_arg_0=6";
    let service = salvo_core::Service::new(router);
    for (path, expected) in [
        (format!("handler?{query}"), "1/2/3/4/5/6"),
        (format!("endpoint?{query}"), "1/2/3/4/5/6"),
        ("same-request?macro_gen_req=request".to_owned(), "request"),
        (
            "same-extractor?macro_gen_arg_0=extractor".to_owned(),
            "extractor",
        ),
        ("generic".to_owned(), "7/9"),
    ] {
        let mut response = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), expected);
    }
    let response = TestClient::get("http://localhost/endpoint?macro_gen_req=invalid")
        .send(&service)
        .await;
    assert_eq!(
        response.status_code,
        Some(salvo_core::http::StatusCode::BAD_REQUEST)
    );
}

mod external_type_macros {
    use std::marker::PhantomData;

    use super::*;

    const __macro_gen_arg_1: usize = 4;
    macro_rules! width {
        () => {
            __macro_gen_arg_1
        };
    }
    pub(super) struct Native<T>(pub(super) PhantomData<T>);
    #[handler]
    impl Native<[u8; width!()]> {
        fn handle(&self, value: QueryParam<[u8; width!()]>) -> String {
            format!("{:?}", value.into_inner())
        }
    }
    pub(super) struct Documented<T>(pub(super) PhantomData<T>);
    #[endpoint]
    impl Documented<[u8; width!()]> {
        fn handle(&self, value: QueryParam<[u8; width!()]>) -> String {
            format!("{:?}", value.into_inner())
        }
    }
}

#[tokio::test]
async fn external_macros_keep_their_constant_names_in_owner_and_extractor_types() {
    use std::marker::PhantomData;
    let router = Router::new()
        .push(Router::with_path("native").get(external_type_macros::Native(PhantomData)))
        .push(Router::with_path("documented").get(external_type_macros::Documented(PhantomData)));
    let doc = serde_json::to_value(OpenApi::new("macros", "1").merge_router(&router)).unwrap();
    assert_eq!(
        doc["paths"]["/documented"]["get"]["parameters"][0]["schema"]["minItems"],
        4
    );
    let service = salvo_core::Service::new(router);
    for path in ["native", "documented"] {
        let mut response = TestClient::get(format!(
            "http://localhost/{path}?value=1&value=2&value=3&value=4"
        ))
        .send(&service)
        .await;
        assert_eq!(response.take_string().await.unwrap(), "[1, 2, 3, 4]");
    }
}
