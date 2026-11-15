//! Cookie mutations preserve wire names and encode values independently.
#![cfg(all(feature = "cookie", feature = "test"))]

use salvo_core::http::cookie::Cookie;
use salvo_core::http::header::SET_COOKIE;
use salvo_core::prelude::*;
use salvo_core::test::TestClient;

const RAW: &str = "name%20with%20spaces";

#[handler]
async fn mutate(req: &mut Request, res: &mut Response) {
    assert_eq!(req.cookie(RAW).unwrap().value(), "old value");
    match req.uri().path() {
        "/remove" => {
            res.remove_cookie(RAW);
        }
        "/remove-with" => {
            res.remove_cookie_with(
                Cookie::build((RAW, ""))
                    .path("/")
                    .domain("example.test")
                    .build(),
            );
        }
        "/jar-remove" => {
            res.cookies_mut().remove(Cookie::new(RAW, ""));
        }
        "/update" => {
            res.add_header(SET_COOKIE, format!("{RAW}=stale; Path=/"), false)
                .unwrap();
            res.add_header(
                SET_COOKIE,
                "name%2520with%2520spaces=unrelated; Path=/",
                false,
            )
            .unwrap();
            res.add_cookie(Cookie::build((RAW, "new value;100%")).path("/").build());
        }
        _ => unreachable!(),
    }
    res.render("done");
}

#[tokio::test]
async fn all_removal_apis_expire_the_actual_wire_cookie() {
    let service = Service::new(Router::with_path("{action}").goal(mutate));
    for path in ["remove", "remove-with", "jar-remove"] {
        let response = TestClient::get(format!("https://example.test/{path}"))
            .add_header("cookie", format!("{RAW}=old%20value"), true)
            .send(&service)
            .await;
        let fields: Vec<_> = response.headers().get_all(SET_COOKIE).iter().collect();
        assert_eq!(fields.len(), 1);
        let wire = fields[0].to_str().unwrap();
        assert!(wire.starts_with(&format!("{RAW}=;")), "{wire}");
        assert!(wire.contains("Max-Age=0"), "{wire}");
        if path == "remove-with" {
            assert!(wire.contains("Path=/"));
            assert!(wire.contains("Domain=example.test"));
        }
    }
}

#[tokio::test]
async fn updates_replace_only_matching_wire_identity_and_encode_the_value() {
    let service = Service::new(Router::with_path("{action}").goal(mutate));
    let response = TestClient::get("https://example.test/update")
        .add_header("cookie", format!("{RAW}=old%20value"), true)
        .send(&service)
        .await;
    let fields: Vec<_> = response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .map(|field| field.to_str().unwrap())
        .collect();
    assert_eq!(fields.len(), 2);
    assert!(fields.contains(&"name%2520with%2520spaces=unrelated; Path=/"));
    assert!(fields.contains(&"name%20with%20spaces=new%20value%3B100%25; Path=/"));
    let serialized = response.into_hyper();
    assert_eq!(serialized.headers().get_all(SET_COOKIE).iter().count(), 2);
}

#[test]
fn new_legal_names_are_wire_tokens_while_invalid_names_keep_escaping() {
    let mut response = Response::new();
    response.add_cookie(Cookie::new("literal%23^|`", "a value"));
    response.add_cookie(Cookie::new("legacy name", "another value"));
    let response = response.into_hyper();
    let fields: Vec<_> = response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .map(|field| field.to_str().unwrap())
        .collect();
    assert!(fields.contains(&"literal%23^|`=a%20value"));
    assert!(fields.contains(&"legacy%20name=another%20value"));
}

#[test]
fn imported_cookie_mutations_preserve_encoded_neighbors_and_attributes() {
    for target in ["name%20with%20spaces", "name%2520with%2520spaces"] {
        let mut source = hyper::Response::new(salvo_core::http::ResBody::None);
        for name in ["name%20with%20spaces", "name%2520with%2520spaces"] {
            source.headers_mut().append(
                SET_COOKIE,
                format!("{name}=old%20value; Path=/; Domain=example.test")
                    .parse()
                    .unwrap(),
            );
        }
        let mut response = Response::from(source);
        assert_eq!(response.cookies().iter().count(), 2);
        let original = response.cookie(target).unwrap().clone();
        assert_eq!(original.value(), "old value");
        response.remove_cookie_with(original);
        let response = response.into_hyper();
        let fields: Vec<_> = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|field| field.to_str().unwrap())
            .collect();
        assert_eq!(fields.len(), 2);
        let removal = fields
            .iter()
            .find(|field| field.starts_with(&format!("{target}=;")))
            .unwrap();
        assert!(removal.contains("Max-Age=0"));
        assert!(removal.contains("Path=/"));
        assert!(removal.contains("Domain=example.test"));
        let neighbor = if target == RAW {
            "name%2520with%2520spaces"
        } else {
            RAW
        };
        assert!(
            fields
                .contains(&format!("{neighbor}=old%20value; Path=/; Domain=example.test").as_str())
        );
    }
}

#[test]
fn explicit_wire_identity_wins_over_its_legacy_invalid_alias() {
    for reverse in [false, true] {
        let mut response = Response::new();
        let mut cookies = [
            Cookie::new("legacy name", "legacy"),
            Cookie::new("legacy%20name", "explicit"),
        ];
        if reverse {
            cookies.reverse();
        }
        for cookie in cookies {
            response.add_cookie(cookie);
        }
        let response = response.into_hyper();
        let fields: Vec<_> = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|field| field.to_str().unwrap())
            .collect();
        assert_eq!(fields, ["legacy%20name=explicit"]);
    }
}
