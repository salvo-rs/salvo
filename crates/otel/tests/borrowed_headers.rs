use opentelemetry::propagation::Extractor;
use opentelemetry_http::HeaderExtractor;
use salvo_core::http::{HeaderMap, HeaderValue};

#[test]
fn propagation_borrows_original_values_and_preserves_repeated_fields() {
    let mut headers = HeaderMap::new();
    headers.append("baggage", HeaderValue::from_static("one=1"));
    headers.append("baggage", HeaderValue::from_static("two=2"));
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer unrelated"),
    );
    let extractor = HeaderExtractor(&headers);
    let values = extractor.get_all("baggage").unwrap();
    assert_eq!(values, ["one=1", "two=2"]);
    assert_eq!(
        values[0].as_ptr(),
        headers["baggage"].to_str().unwrap().as_ptr()
    );
    assert_eq!(extractor.get("missing"), None);
    assert!(extractor.keys().contains(&"baggage"));
}
