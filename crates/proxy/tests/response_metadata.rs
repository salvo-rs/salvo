//! Upstream response metadata must describe the body that is forwarded.
#![allow(missing_docs, clippy::unwrap_used)]

use std::convert::Infallible;

use salvo_core::http::{ReqBody, ResBody};
use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_proxy::{Client, Proxy};

struct Upstream;
impl Client for Upstream {
    type Error = Infallible;
    async fn execute(
        &self,
        _: hyper::Request<ReqBody>,
        _: Option<hyper::upgrade::OnUpgrade>,
    ) -> Result<hyper::Response<ResBody>, Self::Error> {
        Ok(hyper::Response::builder()
            .header("content-type", "text/plain")
            .header("content-length", "5")
            .header("set-cookie", "upstream=1")
            .header("vary", "accept-encoding")
            .body(ResBody::from("hello"))
            .unwrap())
    }
}

#[handler]
async fn prepared(res: &mut Response) {
    res.add_header("content-type", "application/json", true)
        .unwrap();
    res.add_header("content-encoding", "gzip", true).unwrap();
    res.add_header("content-length", "999", true).unwrap();
    res.add_header("content-digest", "old", true).unwrap();
    res.add_header("etag", "old", true).unwrap();
    res.add_header("x-outer", "retained", true).unwrap();
    res.add_header("set-cookie", "outer=1", true).unwrap();
    res.add_header("vary", "origin", true).unwrap();
}

#[tokio::test]
async fn proxy_replaces_stale_representation_metadata() {
    let mut response = TestClient::get("http://localhost/")
        .send(
            Router::new()
                .hoop(prepared)
                .goal(Proxy::new("http://upstream.invalid/", Upstream)),
        )
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(
        response.headers().get_all("content-length").iter().count(),
        1
    );
    assert_eq!(response.headers().get_all("content-type").iter().count(), 1);
    assert_eq!(response.headers()["content-length"], "5");
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert_eq!(response.headers()["x-outer"], "retained");
    assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
    assert_eq!(response.headers().get_all("vary").iter().count(), 2);
    assert!(!response.headers().contains_key("content-digest"));
    assert!(!response.headers().contains_key("etag"));
    assert!(!response.headers().contains_key("content-encoding"));
    let body = response.take_body();
    let ResBody::Once(bytes) = body else {
        panic!("expected plain upstream body")
    };
    assert_eq!(&bytes[..], b"hello");
}
