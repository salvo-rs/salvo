use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_cors::{AllowOrigin, Cors};

#[handler]
async fn hello() -> &'static str {
    "response"
}

#[tokio::test]
async fn outer_policy_replaces_inner_grants_including_none_and_false() {
    let inner = Cors::very_permissive().into_handler();
    let outer = Cors::new()
        .allow_origin(AllowOrigin::list(["https://trusted.example"
            .parse()
            .unwrap()]))
        .allow_credentials(false)
        .into_handler();
    let service = Service::new(Router::new().goal(hello))
        .hoop(outer)
        .hoop(inner);
    for (origin, allowed) in [
        ("https://trusted.example", true),
        ("https://untrusted.example", false),
    ] {
        let response = TestClient::get("https://api.example/")
            .add_header("origin", origin, true)
            .send(&service)
            .await;
        assert_eq!(
            response
                .headers()
                .contains_key("access-control-allow-origin"),
            allowed
        );
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-credentials")
        );
        if allowed {
            assert_eq!(response.headers()["access-control-allow-origin"], origin);
        }
    }
}
