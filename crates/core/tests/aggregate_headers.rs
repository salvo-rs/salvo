//! Aggregate request header extraction preserves repeated values.
#![cfg(feature = "test")]

use salvo_core::http::Request;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct HeaderList<'a> {
    #[serde(rename = "x-tag", borrow)]
    tags: Vec<&'a str>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct HeaderScalar<'a> {
    #[serde(rename = "x-tag", borrow)]
    tag: &'a str,
}

fn request(values: &[&str]) -> Request {
    let mut req = Request::new();
    for value in values {
        req.headers_mut().append("x-tag", value.parse().unwrap());
    }
    req
}

#[test]
fn aggregate_headers_match_direct_sequence_extraction() {
    let mut req = request(&["first", "second"]);
    let expected = req.header::<Vec<String>>("x-tag").unwrap();
    assert_eq!(expected, ["first", "second"]);
    let actual = req.parse_headers::<HeaderList<'_>>().unwrap();
    assert_eq!(actual.tags, expected);
}

#[test]
fn aggregate_headers_use_the_first_value_for_scalar_fields() {
    let mut req = request(&["first", "second"]);
    assert_eq!(req.header::<&str>("x-tag"), Some("first"));
    let actual = req.parse_headers::<HeaderScalar<'_>>().unwrap();
    assert_eq!(actual.tag, "first");
}

#[test]
fn aggregate_headers_accept_a_single_sequence_value() {
    let mut req = request(&["single"]);
    assert_eq!(
        req.parse_headers::<HeaderList<'_>>().unwrap().tags,
        ["single"]
    );
}

#[test]
fn aggregate_headers_keep_ordinary_scalar_values() {
    let mut req = request(&["single"]);
    assert_eq!(
        req.parse_headers::<HeaderScalar<'_>>().unwrap().tag,
        "single"
    );
}

#[test]
fn aggregate_headers_support_typed_values_and_missing_optional_fields() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Values {
        #[serde(rename = "x-number")]
        numbers: Vec<u128>,
        #[serde(rename = "x-enabled")]
        enabled: bool,
        missing: Option<String>,
    }
    let mut req = Request::new();
    req.headers_mut().append("x-number", "1".parse().unwrap());
    req.headers_mut()
        .append("x-number", u128::MAX.to_string().parse().unwrap());
    req.headers_mut()
        .insert("x-enabled", "true".parse().unwrap());
    let result = req.parse_headers::<Values>().unwrap();
    assert_eq!(result.numbers, [1, u128::MAX]);
    assert!(result.enabled);
    assert_eq!(result.missing, None);
}

#[test]
fn aggregate_headers_keep_each_field_value_whole_and_reject_invalid_numbers() {
    let mut req = request(&["first, second", "third"]);
    assert_eq!(
        req.parse_headers::<HeaderList<'_>>().unwrap().tags,
        ["first, second", "third"]
    );

    #[derive(Debug, Deserialize)]
    struct Numbers {
        #[serde(rename = "x-tag")]
        _number: u32,
    }
    assert!(
        request(&["not-a-number"])
            .parse_headers::<Numbers>()
            .is_err()
    );
}
