//! External dependency aliases and standard paths ignore caller-local names.
use salvo_core::Router;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::{Components, OpenApi, ToSchema};
use serde_json::json;

#[allow(dead_code)]
mod shadowed {
    mod salvo_core {}
    mod salvo_oapi {}
    mod std {}
    trait Send {}

    #[derive(::serde::Serialize, ::salvo_oapi::ToSchema)]
    pub(super) struct Vec<T> {
        pub(super) item: T,
    }
    #[derive(::serde::Deserialize, ::salvo_core::macros::Extractible, ::salvo_oapi::ToSchema)]
    struct Input {
        value: String,
    }
    #[derive(::serde::Deserialize, ::salvo_oapi::ToParameters)]
    struct Parameters {
        value: String,
    }
    #[derive(::salvo_oapi::ToResponse)]
    struct Reply(String);
    #[derive(::salvo_oapi::ToResponses)]
    enum Replies {
        #[salvo(response(status_code = 200))]
        Ok(String),
    }
    #[::salvo_core::handler]
    pub(super) fn answer(value: ::salvo_oapi::extract::QueryParam<String>) -> String {
        value.into_inner()
    }
    #[::salvo_oapi::endpoint(status_codes(::salvo_core::http::StatusCode::OK))]
    pub(super) fn documented(value: Parameters) -> String {
        value.value.to_owned()
    }
}

#[tokio::test]
async fn local_names_do_not_capture_generated_paths() {
    let mut components = Components::new();
    let schema = shadowed::Vec::<u32>::to_schema(&mut components);
    assert!(json!(schema).get("$ref").is_some());
    let router = Router::new()
        .push(Router::with_path("answer").get(shadowed::answer))
        .push(Router::with_path("documented").get(shadowed::documented));
    let doc = json!(OpenApi::new("paths", "1").merge_router(&router));
    assert!(doc["paths"]["/documented"]["get"]["responses"]["200"].is_object());
    let service = salvo_core::Service::new(router);
    for path in ["answer", "documented"] {
        assert_eq!(
            TestClient::get(format!("http://localhost/{path}?value=hello"))
                .send(&service)
                .await
                .take_string()
                .await
                .unwrap(),
            "hello"
        );
    }
}
