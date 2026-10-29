//! Trusted JWKS cache policy must not be overwritten by quoted extensions.

#![cfg(feature = "oidc")]

use std::time::Duration;

use salvo_core::http::HeaderValue;
use salvo_jwt_auth::oidc::CachePolicy;

#[test]
fn quoted_extensions_cannot_extend_jwks_cache_policy() {
    let header = HeaderValue::from_static(
        "max-age=1, extension=\"ignored,max-age=2592000,stale-if-error=2592000,ignored\"",
    );
    let policy = CachePolicy::from_header_val(Some(&header));
    assert_eq!(policy.max_age, Duration::from_secs(1));
    assert_eq!(policy.stale_if_error, Some(Duration::from_secs(60)));
}

#[test]
fn directive_names_and_quoted_delta_seconds_follow_http_syntax() {
    let header = HeaderValue::from_static(
        "MAX-AGE=\"120\", Stale-While-Revalidate=\"30\", STALE-IF-ERROR=\"90\"",
    );
    let policy = CachePolicy::from_header_val(Some(&header));
    assert_eq!(policy.max_age, Duration::from_secs(120));
    assert_eq!(policy.stale_while_revalidate, Some(Duration::from_secs(30)));
    assert_eq!(policy.stale_if_error, Some(Duration::from_secs(90)));
}

#[test]
fn opaque_and_escaped_extensions_preserve_real_directives() {
    for header in [
        HeaderValue::from_bytes(b"extension=\"opaque\x80,max-age=2592000,ignored\", max-age=60")
            .unwrap(),
        HeaderValue::from_static(r#"max-age=60, extension="quote\",max-age=2592000,ignored""#),
        HeaderValue::from_static(r#"max-age=60, extension="slash\\", stale-if-error=90"#),
    ] {
        assert_eq!(
            CachePolicy::from_header_val(Some(&header)).max_age,
            Duration::from_secs(60)
        );
    }
    let header = HeaderValue::from_static(r#"max-age="\1\2", stale-if-error=0"#);
    let policy = CachePolicy::from_header_val(Some(&header));
    assert_eq!(policy.max_age, Duration::from_secs(12));
    assert_eq!(policy.stale_if_error, Some(Duration::ZERO));
}

#[test]
fn valid_bounds_and_invalid_delta_seconds_keep_conservative_defaults() {
    for value in [
        "+120",
        "-120",
        "120=999",
        "\"120",
        "\"12x\"",
        "\"\"",
        "18446744073709551616",
    ] {
        let header = HeaderValue::from_str(&format!("max-age={value}")).unwrap();
        assert_eq!(
            CachePolicy::from_header_val(Some(&header)).max_age,
            Duration::from_secs(1),
            "{value}"
        );
    }
    let zero = HeaderValue::from_static("max-age=0, stale-while-revalidate=0");
    let policy = CachePolicy::from_header_val(Some(&zero));
    assert_eq!(policy.max_age, Duration::from_secs(1));
    assert_eq!(policy.stale_while_revalidate, Some(Duration::ZERO));
    let huge = HeaderValue::from_static(
        "max-age=18446744073709551615, stale-if-error=18446744073709551615",
    );
    let policy = CachePolicy::from_header_val(Some(&huge));
    assert_eq!(policy.max_age, Duration::from_secs(30 * 24 * 60 * 60));
    assert_eq!(policy.stale_if_error, Some(policy.max_age));
}
