//! Existing standard StatusCode constants are accepted and inferred.
#![allow(unused_braces)]
use salvo_core::Router;
use salvo_core::http::StatusCode;
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

#[endpoint(responses((status_code = StatusCode::EARLY_HINTS, description = "Hints"), (status_code = StatusCode::TOO_EARLY, description = "Retry")))]
fn explicit() {}

#[endpoint]
fn inferred() -> StatusCode {
    StatusCode::TOO_EARLY
}

#[test]
fn standard_constant_forms_match_their_numeric_codes() {
    let document = json!(OpenApi::new("status", "1").merge_router(&Router::new().get(explicit)));
    assert_eq!(
        document["paths"]["/"]["get"]["responses"]["103"]["description"],
        "Hints"
    );
    assert_eq!(
        document["paths"]["/"]["get"]["responses"]["425"]["description"],
        "Retry"
    );
}

#[test]
fn inferred_status_code_responses_cover_all_standard_statuses() {
    let document = json!(OpenApi::new("status", "1").merge_router(&Router::new().get(inferred)));
    let responses = document["paths"]["/"]["get"]["responses"]
        .as_object()
        .unwrap();
    assert_eq!(responses.len(), 62);
    for code in ["103", "425", "418"] {
        assert!(responses.contains_key(code), "missing {code}");
    }
}
