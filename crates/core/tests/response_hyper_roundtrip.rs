//! HTTP metadata and Set-Cookie survive Salvo/Hyper and test serialization.
#![cfg(feature = "test")]

use salvo_core::http::{ResBody, Version};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[derive(Clone, Debug, PartialEq)]
struct Marker(u32);

#[tokio::test]
async fn roundtrip_preserves_version_extensions_headers_status_and_body() {
    let mut source = hyper::Response::builder()
        .status(201)
        .version(Version::HTTP_10)
        .header("x-marker", "header")
        .body(ResBody::from("hello"))
        .unwrap();
    source.extensions_mut().insert(Marker(7));
    let mut response = Response::from(source);
    assert_eq!(response.extensions.get::<Marker>(), Some(&Marker(7)));
    let outgoing = response.strip_to_hyper();
    assert_eq!(outgoing.version(), Version::HTTP_10);
    assert_eq!(outgoing.extensions().get::<Marker>(), Some(&Marker(7)));
    let outgoing = Response::from(outgoing).into_hyper();
    assert_eq!(outgoing.version(), Version::HTTP_10);
    assert_eq!(outgoing.extensions().get::<Marker>(), Some(&Marker(7)));
    assert_eq!(outgoing.status(), StatusCode::CREATED);
    assert_eq!(outgoing.headers()["x-marker"], "header");
    assert_eq!(
        Response::from(outgoing).take_string().await.unwrap(),
        "hello"
    );
}

#[cfg(feature = "cookie")]
#[test]
fn unchanged_set_cookie_headers_are_emitted_once_and_byte_preserved() {
    let original = [
        "sid=first; Path=/a; HttpOnly",
        "sid=second; Path=/b; SameSite=Lax",
        "other=value%20with%20spaces; Domain=example.com",
    ];
    let mut source = hyper::Response::builder().body(ResBody::None).unwrap();
    for value in original {
        source
            .headers_mut()
            .append("set-cookie", value.parse().unwrap());
    }
    let mut response = Response::from(source);
    assert!(!response.has_cookie_changes());
    let outgoing = response.strip_to_hyper();
    let outgoing = Response::from(outgoing).into_hyper();
    let values: Vec<_> = outgoing
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(values, original);
}

#[cfg(feature = "cookie")]
#[test]
fn cookie_changes_replace_only_the_matching_name_domain_and_path() {
    use cookie::Cookie;
    let source = hyper::Response::builder()
        .header("set-cookie", "sid=old; Path=/a; Domain=EXAMPLE.com")
        .header("set-cookie", "sid=other-path; Path=/b; Domain=example.com")
        .header(
            "set-cookie",
            "sid=other-domain; Path=/a; Domain=other.example",
        )
        .header("set-cookie", "opaque-field")
        .body(ResBody::None)
        .unwrap();
    let mut response = Response::from(source);
    response.add_cookie(
        Cookie::build(("sid", "new"))
            .path("/a")
            .domain("example.com")
            .build(),
    );
    let outgoing = response.into_hyper();
    let values: Vec<_> = outgoing
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(values.len(), 4);
    assert!(!values.iter().any(|value| value.starts_with("sid=old;")));
    assert!(values.iter().any(|value| value.starts_with("sid=new;")));
    assert!(values.contains(&"sid=other-path; Path=/b; Domain=example.com"));
    assert!(values.contains(&"sid=other-domain; Path=/a; Domain=other.example"));
    assert!(values.contains(&"opaque-field"));

    let mut response = Response::from(outgoing);
    response.remove_cookie_with(
        Cookie::build("sid")
            .path("/a")
            .domain("example.com")
            .build(),
    );
    let outgoing = response.into_hyper();
    let values: Vec<_> = outgoing
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(values.len(), 4);
    assert!(!values.iter().any(|value| value.starts_with("sid=new;")));
    assert!(
        values
            .iter()
            .any(|value| value.starts_with("sid=;") && value.contains("Max-Age=0"))
    );
}

#[cfg(feature = "cookie")]
#[tokio::test]
async fn test_client_exposes_every_cookie_and_can_serialize_again_without_duplicates() {
    #[handler]
    async fn cookies(res: &mut Response) {
        res.add_cookie(cookie::Cookie::new("session", "one"));
        res.add_cookie(cookie::Cookie::new("csrf", "two"));
        res.render("ok");
    }
    let response = TestClient::get("http://localhost/")
        .send(Router::new().get(cookies))
        .await;
    assert_eq!(response.cookie("session").unwrap().value(), "one");
    assert_eq!(response.cookie("csrf").unwrap().value(), "two");
    assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
    assert!(!response.has_cookie_changes());
    assert_eq!(
        response
            .into_hyper()
            .headers()
            .get_all("set-cookie")
            .iter()
            .count(),
        2
    );
}
