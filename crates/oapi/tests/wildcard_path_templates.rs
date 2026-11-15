//! A wildcard route is documented with the name its path parameter has.

use salvo_core::Router;
use salvo_core::http::StatusCode;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::extract::PathParam;
use salvo_oapi::{OpenApi, endpoint};

#[endpoint]
async fn show(rest: PathParam<String>) -> String {
    rest.into_inner()
}

/// The names used in a path template, in order.
fn template_names(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|part| part.split('}').next())
        .collect()
}

#[test]
fn template_expressions_match_the_declared_path_parameters() {
    for pattern in [
        "files/{**rest}",
        "files/{*+rest}",
        "files/{*?rest}",
        "files/{*rest}",
        "files/{rest}",
        "files/{rest:num}",
    ] {
        let router = Router::new().push(Router::with_path(pattern).get(show));
        let document =
            serde_json::to_value(OpenApi::new("test", "0.0.1").merge_router(&router)).unwrap();
        let paths = document["paths"].as_object().unwrap();
        assert_eq!(
            paths.keys().collect::<Vec<_>>(),
            ["/files/{rest}"],
            "{pattern}"
        );
        for (path, item) in paths {
            let declared = item["get"]["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|parameter| parameter["in"] == "path")
                .map(|parameter| parameter["name"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(template_names(path), declared, "{pattern}");
        }
    }
}

#[test]
fn wildcard_and_plain_routes_share_one_path_item() {
    let router = Router::new()
        .push(Router::with_path("files/{**rest}").get(show))
        .push(Router::with_path("files/{rest}").post(show));
    let document =
        serde_json::to_value(OpenApi::new("test", "0.0.1").merge_router(&router)).unwrap();
    let paths = document["paths"].as_object().unwrap();
    assert_eq!(paths.keys().collect::<Vec<_>>(), ["/files/{rest}"]);
    assert!(paths["/files/{rest}"]["get"].is_object());
    assert!(paths["/files/{rest}"]["post"].is_object());
}

#[tokio::test]
async fn wildcard_parameter_is_named_without_its_prefix_at_runtime() {
    let service = Service::new(Router::new().push(Router::with_path("files/{**rest}").get(show)));
    let mut response = TestClient::get("http://localhost/files/a/b.txt")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.take_string().await.unwrap(), "a/b.txt");
}
