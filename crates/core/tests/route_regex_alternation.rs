//! A regex path parameter with alternatives must match the whole segment or rest.
#![cfg(all(feature = "test", feature = "routing-regex"))]

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn show(req: &mut Request) -> String {
    let mut params = req
        .params()
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    params.sort();
    params.join(",")
}

async fn get(service: &Service, path: &str) -> (Option<StatusCode>, String) {
    let mut response = TestClient::get(format!("http://localhost{path}"))
        .send(service)
        .await;
    let status = response.status_code;
    let body = if status == Some(StatusCode::OK) {
        response.take_string().await.unwrap()
    } else {
        String::new()
    };
    (status, body)
}

#[tokio::test]
async fn every_alternative_matches_a_whole_segment() {
    let service = Service::new(
        Router::new()
            .push(Router::with_path("export/{fmt|js|json}").get(show))
            .push(Router::with_path("locale/{lang|en|en-US}").get(show)),
    );
    for (path, expected) in [
        ("/export/js", "fmt=js"),
        ("/export/json", "fmt=json"),
        ("/locale/en", "lang=en"),
        ("/locale/en-US", "lang=en-US"),
    ] {
        assert_eq!(
            get(&service, path).await,
            (Some(StatusCode::OK), expected.to_owned()),
            "{path}"
        );
    }
    for path in [
        "/export/jso",
        "/export/xjs",
        "/export/jsonx",
        "/locale/en-",
        "/locale/xen-US",
    ] {
        assert_eq!(
            get(&service, path).await.0,
            Some(StatusCode::NOT_FOUND),
            "{path}"
        );
    }
}

#[tokio::test]
async fn every_alternative_matches_the_whole_rest() {
    let service = Service::new(Router::with_path("files/{**path|public/.*|assets/.*}").get(show));
    for (path, expected) in [
        ("/files/public/a.txt", "path=public/a.txt"),
        ("/files/assets/a/b.txt", "path=assets/a/b.txt"),
    ] {
        assert_eq!(
            get(&service, path).await,
            (Some(StatusCode::OK), expected.to_owned()),
            "{path}"
        );
    }
    for path in [
        "/files/private/a.txt",
        "/files/x/assets/abc",
        "/files/admin/assets/a/b/c/d/e",
        "/files/xpublic/a.txt",
    ] {
        assert_eq!(
            get(&service, path).await.0,
            Some(StatusCode::NOT_FOUND),
            "{path}"
        );
    }
}

#[tokio::test]
async fn verbose_trailing_comments_do_not_swallow_the_added_group() {
    for (pattern, accepted, expected) in [
        (
            r"comments/{id|(?x)^foo\|bar$ # trailing comment}",
            "/comments/foo%7Cbar",
            "id=foo|bar",
        ),
        (
            r"comments/{id|(?x)foo|foobar # trailing comment}",
            "/comments/foobar",
            "id=foobar",
        ),
        (
            r"comments/{id|(?x)[a|b]+ # trailing comment}",
            "/comments/a%7Cb",
            "id=a|b",
        ),
    ] {
        let service = Service::new(Router::try_with_path(pattern).unwrap().get(show));
        assert_eq!(
            get(&service, accepted).await,
            (Some(StatusCode::OK), expected.to_owned()),
            "{pattern}"
        );
        assert_eq!(
            get(&service, "/comments/not-a-match").await.0,
            Some(StatusCode::NOT_FOUND),
            "{pattern}"
        );
    }
    assert!(Router::try_with_path("invalid/{id|foo|[bar}").is_err());
}
