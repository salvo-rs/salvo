//! Craft-created macro paths ignore a same-named caller-local framework module.
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
mod shadowed {
    mod salvo {}
    mod salvo_core {}
    mod salvo_oapi {}
    #[derive(Clone)]
    pub(super) struct Service;
    #[::salvo_craft::craft]
    impl Service {
        #[craft(handler)]
        pub(super) fn plain(&self) -> &'static str {
            "ok"
        }
        #[craft(endpoint)]
        pub(super) fn documented(&self) -> &'static str {
            "ok"
        }
    }
}
#[tokio::test]
async fn crafted_dependency_paths_are_absolute() {
    let router = Router::new()
        .push(Router::with_path("plain").get(shadowed::Service.plain()))
        .push(Router::with_path("documented").get(shadowed::Service.documented()));
    let document = salvo::oapi::OpenApi::new("paths", "1").merge_router(&router);
    assert!(document.paths.contains_key("/documented"));
    let server = salvo::Service::new(router);
    for path in ["plain", "documented"] {
        assert_eq!(
            TestClient::get(format!("http://localhost/{path}"))
                .send(&server)
                .await
                .take_string()
                .await
                .unwrap(),
            "ok"
        );
    }
}
