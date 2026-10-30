//! JWT finder defaults and explicit allowlists cover extension HTTP methods.

use salvo_core::http::Method;
use salvo_core::test::RequestBuilder;
use salvo_jwt_auth::{CookieFinder, FormFinder, HeaderFinder, JwtTokenFinder, QueryFinder};

fn request(method: Method) -> salvo_core::Request {
    RequestBuilder::new("http://localhost/?access_token=token", method)
        .add_header("authorization", "Bearer token", true)
        .add_header("cookie", "access_token=token", true)
        .raw_form("access_token=token")
        .build()
}

fn default_finders() -> Vec<Box<dyn JwtTokenFinder>> {
    vec![
        Box::new(HeaderFinder::default()),
        Box::new(QueryFinder::new("access_token")),
        Box::new(CookieFinder::new("access_token")),
        Box::new(FormFinder::new("access_token")),
    ]
}

#[tokio::test]
async fn default_finders_accept_standard_and_extension_methods() {
    for finder in default_finders() {
        for method in ["GET", "POST", "QUERY", "PROPFIND", "CUSTOM"] {
            assert_eq!(
                finder
                    .find_token(&mut request(method.parse().unwrap()))
                    .await
                    .as_deref(),
                Some("token"),
                "default finder must accept {method}"
            );
        }
    }
}

#[tokio::test]
async fn explicit_method_lists_restrict_every_finder() {
    let extension: Method = "PROPFIND".parse().unwrap();
    for allowed in [vec![], vec![Method::GET], vec![extension.clone()]] {
        let finders: Vec<Box<dyn JwtTokenFinder>> = vec![
            Box::new(HeaderFinder::new().allowed_methods(allowed.clone())),
            Box::new(QueryFinder::new("access_token").allowed_methods(allowed.clone())),
            Box::new(CookieFinder::new("access_token").allowed_methods(allowed.clone())),
            Box::new(FormFinder::new("access_token").allowed_methods(allowed.clone())),
        ];
        for finder in finders {
            for method in [Method::GET, Method::POST, extension.clone()] {
                let expected = allowed.contains(&method).then_some("token");
                assert_eq!(
                    finder
                        .find_token(&mut request(method.clone()))
                        .await
                        .as_deref(),
                    expected,
                    "explicit list {allowed:?} must restrict {method}"
                );
            }
        }
    }
}

#[tokio::test]
async fn mutable_method_lists_keep_empty_and_extension_semantics() {
    let extension: Method = "PROPFIND".parse().unwrap();
    let mut header = HeaderFinder::new();
    let mut query = QueryFinder::new("access_token");
    let mut cookie = CookieFinder::new("access_token");
    let mut form = FormFinder::new("access_token");
    header.allowed_methods_mut().clear();
    query.allowed_methods_mut().clear();
    cookie.allowed_methods_mut().clear();
    form.allowed_methods_mut().clear();
    for finder in [&header as &dyn JwtTokenFinder, &query, &cookie, &form] {
        assert_eq!(finder.find_token(&mut request(Method::GET)).await, None);
        assert_eq!(
            finder.find_token(&mut request(extension.clone())).await,
            None
        );
    }
    header.allowed_methods_mut().push(extension.clone());
    query.allowed_methods_mut().push(extension.clone());
    cookie.allowed_methods_mut().push(extension.clone());
    form.allowed_methods_mut().push(extension.clone());
    for finder in [&header as &dyn JwtTokenFinder, &query, &cookie, &form] {
        assert_eq!(finder.find_token(&mut request(Method::GET)).await, None);
        assert_eq!(
            finder
                .find_token(&mut request(extension.clone()))
                .await
                .as_deref(),
            Some("token")
        );
    }
}

#[tokio::test]
async fn mutable_access_materializes_a_list_and_none_restores_all_methods() {
    let extension: Method = "PROPFIND".parse().unwrap();
    macro_rules! check {
        ($finder:expr) => {{
            let mut finder = $finder;
            assert!(finder.allowed_methods.is_none());
            assert!(finder.allowed_methods_mut().contains(&Method::GET));
            assert_eq!(
                finder.find_token(&mut request(extension.clone())).await,
                None
            );
            finder.allowed_methods = None;
            assert_eq!(
                finder
                    .find_token(&mut request(extension.clone()))
                    .await
                    .as_deref(),
                Some("token")
            );
            finder.allowed_methods = Some(vec![]);
            assert_eq!(
                finder.find_token(&mut request(extension.clone())).await,
                None
            );
            finder.allowed_methods = Some(vec![extension.clone()]);
            assert_eq!(
                finder
                    .find_token(&mut request(extension.clone()))
                    .await
                    .as_deref(),
                Some("token")
            );
            assert_eq!(finder.find_token(&mut request(Method::GET)).await, None);
        }};
    }
    check!(HeaderFinder::new());
    check!(QueryFinder::new("access_token"));
    check!(CookieFinder::new("access_token"));
    check!(FormFinder::new("access_token"));
}

#[test]
fn default_settings_are_unrestricted() {
    assert_eq!(HeaderFinder::default(), HeaderFinder::new());
    assert_eq!(QueryFinder::default(), QueryFinder::new(""));
    assert_eq!(CookieFinder::default(), CookieFinder::new(""));
    assert_eq!(FormFinder::default(), FormFinder::new(""));
}
