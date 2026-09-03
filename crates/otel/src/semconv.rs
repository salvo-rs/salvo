//! Helpers shared by the [`Metrics`](crate::Metrics) and
//! [`Tracing`](crate::Tracing) middlewares for producing attribute values that
//! follow the OpenTelemetry semantic conventions.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use opentelemetry::Value;
use salvo_core::http::uri::Scheme;
use salvo_core::http::{Method, StatusCode, Version};
use salvo_core::{Request, Response};

/// Sentinel the conventions define for an `http.request.method` outside the
/// known set. This crate also uses it for a `url.scheme` outside its own known
/// set, for the same reason: keeping a client from choosing the value.
pub(crate) const OTHER: &str = "_OTHER";

/// Value substituted for a redacted query-string value.
const REDACTED_VALUE: &str = "REDACTED";

/// Query-string keys whose values the HTTP semantic conventions require to be
/// redacted before `url.query` is recorded.
const REDACTED_QUERY_KEYS: [&str; 4] = ["AWSAccessKeyId", "Signature", "sig", "X-Goog-Signature"];

/// A set of method names an application declared known, overriding the default.
pub(crate) type KnownMethods = HashSet<Arc<str>>;

/// Collects `methods` into the set accepted by [`known_method_value`].
pub(crate) fn known_methods<I, S>(methods: I) -> KnownMethods
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    methods
        .into_iter()
        .map(|method| Arc::from(method.as_ref()))
        .collect()
}

/// Returns the `http.request.method` value for `method`, or `None` when the
/// method is not known and has to be reported as [`OTHER`].
///
/// `known` overrides the default set, which is the methods RFC 9110 defines
/// plus `PATCH` from RFC 5789. Method names are matched case-sensitively, as
/// the conventions require.
pub(crate) fn known_method_value(method: &Method, known: Option<&KnownMethods>) -> Option<Value> {
    match known {
        Some(known) => known
            .get(method.as_str())
            .map(|method| Value::from(Arc::clone(method))),
        None => default_known_method(method.as_str()).map(Value::from),
    }
}

/// Returns `method` itself when it is one of the methods defined by RFC 9110 or
/// RFC 5789, so the value can be recorded without allocating.
fn default_known_method(method: &str) -> Option<&'static str> {
    Some(match method {
        "GET" => "GET",
        "HEAD" => "HEAD",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "CONNECT" => "CONNECT",
        "OPTIONS" => "OPTIONS",
        "TRACE" => "TRACE",
        "PATCH" => "PATCH",
        _ => return None,
    })
}

/// Returns the `url.scheme` value for `scheme`.
pub(crate) fn scheme_value(scheme: &Scheme) -> Cow<'static, str> {
    match known_scheme(scheme) {
        Some(scheme) => Cow::Borrowed(scheme),
        None => Cow::Owned(scheme.as_str().to_owned()),
    }
}

/// Returns the `url.scheme` value for `scheme`, drawn from a bounded set.
///
/// A request target may be given in absolute form, and salvo takes the scheme
/// from it in preference to the one the connection was accepted on, so the
/// value is client-controlled. Recording it verbatim would let a client open a
/// time series per scheme it invents, so a scheme a salvo server cannot
/// actually have spoken is reported as [`OTHER`], the way an unknown method is.
/// Spans, which are not aggregated by attribute value, keep the scheme sent.
pub(crate) fn bounded_scheme_value(scheme: &Scheme) -> &'static str {
    known_scheme(scheme).unwrap_or(OTHER)
}

/// Returns the schemes a salvo server speaks, so they can be recorded without
/// allocating.
fn known_scheme(scheme: &Scheme) -> Option<&'static str> {
    if scheme == &Scheme::HTTP {
        Some("http")
    } else if scheme == &Scheme::HTTPS {
        Some("https")
    } else {
        None
    }
}

/// Returns the `network.protocol.version` value for `version`.
///
/// The conventions spell the version out as `1.1` or `2`, not in the `HTTP/1.1`
/// form that [`Version`]'s `Debug` output uses. Versions the conventions do not
/// name return `None`, so the attribute is left out instead of guessed.
pub(crate) fn protocol_version(version: Version) -> Option<&'static str> {
    if version == Version::HTTP_11 {
        Some("1.1")
    } else if version == Version::HTTP_2 {
        Some("2")
    } else if version == Version::HTTP_3 {
        Some("3")
    } else if version == Version::HTTP_10 {
        Some("1.0")
    } else if version == Version::HTTP_09 {
        Some("0.9")
    } else {
        None
    }
}

/// Returns the `http.route` value for `req`, or `None` when no route matched
/// and the conventions ask for the attribute to be left out.
///
/// Salvo reports a matched route without the leading separator the conventions
/// expect (`users/{id}`), and reports a goal mounted at the router root as an
/// empty string — the same value a request that matched nothing carries. The
/// requested path tells the two apart: `/` reached the root route, and any
/// other path with nothing matched reached no route at all.
///
/// The two cases overlap only for a request to `/` that matched nothing, which
/// happens when the application has no root route — and then there is no root
/// traffic for it to be confused with.
#[cfg(feature = "matched-path")]
pub(crate) fn route_value(req: &Request) -> Option<String> {
    let matched_path = req.matched_path();
    if matched_path.is_empty() {
        return (req.uri().path() == "/").then(|| "/".to_owned());
    }
    let mut route = String::with_capacity(matched_path.len() + 1);
    route.push('/');
    route.push_str(matched_path);
    Some(route)
}

/// Returns the `http.response.body.size` value when middleware can already tell
/// what will be sent.
///
/// The service finishes a response after middleware returns: a `4xx`/`5xx` with
/// no body yet is handed to the catcher, and a `HEAD` response has its body
/// stripped. In both cases the body observable here is not the one that goes on
/// the wire, so nothing is reported rather than a wrong size.
pub(crate) fn final_body_size(req: &Request, res: &Response, status: StatusCode) -> Option<u64> {
    if *req.method() == Method::HEAD {
        return None;
    }
    if (status.is_client_error() || status.is_server_error()) && res.body.is_none() {
        return None;
    }
    res.body.size()
}

/// Replaces the values of well-known credential-bearing query keys with
/// `REDACTED`, keeping their keys, as the conventions require before
/// `url.query` is recorded.
pub(crate) fn redact_query(query: &str) -> Cow<'_, str> {
    if !needs_redaction(query) {
        return Cow::Borrowed(query);
    }
    let mut redacted = String::with_capacity(query.len());
    for (idx, pair) in query.split('&').enumerate() {
        if idx > 0 {
            redacted.push('&');
        }
        match pair.split_once('=') {
            Some((key, _)) if REDACTED_QUERY_KEYS.contains(&key) => {
                redacted.push_str(key);
                redacted.push('=');
                redacted.push_str(REDACTED_VALUE);
            }
            _ => redacted.push_str(pair),
        }
    }
    Cow::Owned(redacted)
}

/// Returns whether `query` carries any key whose value has to be redacted, so
/// the common case can borrow the query instead of rebuilding it.
fn needs_redaction(query: &str) -> bool {
    query.split('&').any(|pair| {
        pair.split_once('=')
            .is_some_and(|(key, _)| REDACTED_QUERY_KEYS.contains(&key))
    })
}

#[cfg(test)]
mod tests {
    use salvo_core::http::ResBody;

    use super::*;

    #[test]
    fn test_known_method_value_defaults() {
        for name in [
            "GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH",
        ] {
            let method = Method::from_bytes(name.as_bytes()).expect("valid method");
            assert_eq!(known_method_value(&method, None), Some(Value::from(name)));
        }
    }

    #[test]
    fn test_known_method_value_is_case_sensitive() {
        let method = Method::from_bytes(b"get").expect("valid method");
        assert_eq!(known_method_value(&method, None), None);
    }

    #[test]
    fn test_known_method_value_unknown() {
        let method = Method::from_bytes(b"PROPFIND").expect("valid method");
        assert_eq!(known_method_value(&method, None), None);
    }

    #[test]
    fn test_known_method_value_override() {
        let known = known_methods(["GET", "PROPFIND"]);
        let propfind = Method::from_bytes(b"PROPFIND").expect("valid method");
        assert_eq!(
            known_method_value(&propfind, Some(&known)),
            Some(Value::from("PROPFIND"))
        );
        // The override replaces the default set instead of extending it.
        assert_eq!(known_method_value(&Method::POST, Some(&known)), None);
    }

    #[test]
    fn test_scheme_value() {
        assert_eq!(scheme_value(&Scheme::HTTP), "http");
        assert_eq!(scheme_value(&Scheme::HTTPS), "https");
        let ws: Scheme = "ws".parse().expect("valid scheme");
        assert_eq!(scheme_value(&ws), "ws");
    }

    #[test]
    fn test_bounded_scheme_value() {
        assert_eq!(bounded_scheme_value(&Scheme::HTTP), "http");
        assert_eq!(bounded_scheme_value(&Scheme::HTTPS), "https");
        // An absolute-form request target lets a client pick the scheme, so it
        // must not reach a metric dimension verbatim.
        let invented: Scheme = "salvo-is-great".parse().expect("valid scheme");
        assert_eq!(bounded_scheme_value(&invented), "_OTHER");
    }

    #[test]
    fn test_protocol_version() {
        assert_eq!(protocol_version(Version::HTTP_09), Some("0.9"));
        assert_eq!(protocol_version(Version::HTTP_10), Some("1.0"));
        assert_eq!(protocol_version(Version::HTTP_11), Some("1.1"));
        assert_eq!(protocol_version(Version::HTTP_2), Some("2"));
        assert_eq!(protocol_version(Version::HTTP_3), Some("3"));
    }

    #[cfg(feature = "matched-path")]
    #[test]
    fn test_route_value() {
        let mut req = Request::new();
        *req.uri_mut() = "/users/42".parse().expect("valid uri");
        *req.matched_path_mut() = "users/{id}".to_owned();
        assert_eq!(route_value(&req).as_deref(), Some("/users/{id}"));
    }

    #[cfg(feature = "matched-path")]
    #[test]
    fn test_route_value_root_goal() {
        let mut req = Request::new();
        *req.uri_mut() = "/".parse().expect("valid uri");
        // Salvo reports a goal mounted at the router root with no matched parts.
        assert_eq!(route_value(&req).as_deref(), Some("/"));
    }

    #[cfg(feature = "matched-path")]
    #[test]
    fn test_route_value_unmatched() {
        let mut req = Request::new();
        *req.uri_mut() = "/nowhere".parse().expect("valid uri");
        assert_eq!(
            route_value(&req),
            None,
            "an unmatched request carries no route rather than its raw path"
        );
    }

    #[test]
    fn test_final_body_size_skips_head() {
        let mut req = Request::new();
        *req.method_mut() = Method::HEAD;
        let mut res = Response::new();
        res.body = ResBody::Once("Hello".into());
        assert_eq!(final_body_size(&req, &res, StatusCode::OK), None);
    }

    #[test]
    fn test_final_body_size_skips_body_left_to_catcher() {
        let req = Request::new();
        let res = Response::new();
        assert_eq!(
            final_body_size(&req, &res, StatusCode::NOT_FOUND),
            None,
            "an empty error response is filled in after middleware returns"
        );
    }

    #[test]
    fn test_final_body_size_reports_buffered_body() {
        let req = Request::new();
        let mut res = Response::new();
        res.body = ResBody::Once("Hello".into());
        assert_eq!(final_body_size(&req, &res, StatusCode::OK), Some(5));
    }

    #[test]
    fn test_final_body_size_reports_empty_success() {
        let req = Request::new();
        let res = Response::new();
        assert_eq!(final_body_size(&req, &res, StatusCode::NO_CONTENT), Some(0));
    }

    #[test]
    fn test_redact_query_keeps_untouched_query_borrowed() {
        let query = "q=OpenTelemetry&page=2";
        assert!(matches!(redact_query(query), Cow::Borrowed(_)));
        assert_eq!(redact_query(query), query);
    }

    #[test]
    fn test_redact_query_redacts_known_keys() {
        assert_eq!(
            redact_query("q=OpenTelemetry&sig=abc123"),
            "q=OpenTelemetry&sig=REDACTED"
        );
        assert_eq!(
            redact_query("AWSAccessKeyId=AKIA&Signature=xyz&X-Goog-Signature=zzz"),
            "AWSAccessKeyId=REDACTED&Signature=REDACTED&X-Goog-Signature=REDACTED"
        );
    }

    #[test]
    fn test_redact_query_leaves_similar_keys_alone() {
        // `sig` matches exactly; `signal` and `design` only contain it.
        assert_eq!(redact_query("signal=1&design=2"), "signal=1&design=2");
    }

    #[test]
    fn test_redact_query_keeps_valueless_pairs() {
        assert_eq!(redact_query("sig=a&flag&b=2"), "sig=REDACTED&flag&b=2");
    }
}
