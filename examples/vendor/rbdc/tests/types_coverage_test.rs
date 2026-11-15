//! Additional coverage for the `rbdc::types` serde error paths and less common
//! conversions.

use std::str::FromStr;

use rbdc::types::{Bytes, DateTime, Decimal, Json, JsonV, Timestamp};
use rbs::Value;

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Payload {
    a: i32,
}

#[test]
fn bytes_from_vec_uses_trait_impl() {
    // The inherent `Bytes::from` shadows the trait method; use `Into` to
    // exercise `impl From<Vec<u8>> for Bytes`.
    let bytes: Bytes = vec![1u8, 2, 3].into();
    assert_eq!(bytes.as_ref(), &[1, 2, 3]);
    assert_eq!(bytes.into_inner(), vec![1, 2, 3]);
}

#[test]
fn decimal_deserialize_ext_and_error_paths() {
    let decimal: Decimal = rbs::from_value(Value::Ext(
        "Decimal",
        Box::new(Value::String("1.5".into())),
    ))
    .unwrap();
    assert_eq!(decimal, Decimal::new("1.5").unwrap());

    assert!(rbs::from_value::<Decimal>(Value::String("not-a-number".into())).is_err());
    assert!(rbs::from_value::<Decimal>(Value::Ext(
        "Decimal",
        Box::new(Value::String("not-a-number".into()))
    ))
    .is_err());
}

#[test]
fn timestamp_wrong_type_errors() {
    assert!(rbs::from_value::<Timestamp>(Value::String("not-a-number".into())).is_err());
    assert!(serde_json::from_value::<Timestamp>(serde_json::json!([])).is_err());
}

#[test]
fn datetime_string_deserialization_paths() {
    let dt = DateTime::from_str("2024-01-15T00:00:00Z").unwrap();
    assert_eq!(
        rbs::from_value::<DateTime>(Value::String(dt.to_string())).unwrap(),
        dt
    );
    assert!(rbs::from_value::<DateTime>(Value::String("not-a-date".into())).is_err());
}

#[test]
fn json_deserialize_error_and_success() {
    assert!(serde_json::from_str::<Json>("not json").is_err());
    let json: Json = serde_json::from_str("null").unwrap();
    assert_eq!(json.0, "null");
}

#[test]
fn jsonv_rbs_error_paths() {
    // Invalid UTF-8 in a binary payload.
    assert!(rbs::from_value::<JsonV<Payload>>(Value::Binary(vec![0xff, 0xfe])).is_err());
    // Valid UTF-8 binary that is not valid JSON.
    assert!(rbs::from_value::<JsonV<Payload>>(Value::Binary(b"not json".to_vec())).is_err());
    // Plain string that is not valid JSON.
    assert!(rbs::from_value::<JsonV<Payload>>(Value::String("not json".into())).is_err());
    // Non-string values are stringified before parsing.
    assert!(rbs::from_value::<JsonV<Payload>>(Value::I32(1)).is_err());
    // Extension values are unwrapped transparently.
    assert!(rbs::from_value::<JsonV<Payload>>(Value::Ext(
        "Json",
        Box::new(Value::String("not json".into()))
    ))
    .is_err());
    assert!(rbs::from_value::<JsonV<Payload>>(Value::Ext(
        "Json",
        Box::new(Value::Binary(vec![0xff]))
    ))
    .is_err());

    // Valid JSON at the top level still works.
    let ok: JsonV<Payload> = rbs::from_value(Value::String(r#"{"a":7}"#.into())).unwrap();
    assert_eq!(ok, JsonV(Payload { a: 7 }));
}

#[test]
fn jsonv_serialize_error_is_reported() {
    #[derive(serde::Deserialize)]
    struct FailsToSerialize;

    impl serde::Serialize for FailsToSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("cannot serialize"))
        }
    }

    // The rbs serializer path first serializes the inner value to a JSON
    // string; a failing inner serializer surfaces as an rbs error.
    let err = rbs::value(&JsonV(FailsToSerialize)).unwrap_err();
    assert!(err.to_string().contains("cannot serialize"));
}

#[test]
fn jsonv_serde_json_error_and_success_paths() {
    assert!(serde_json::from_str::<JsonV<Payload>>("not json").is_err());
    assert!(serde_json::from_str::<JsonV<Payload>>(r#"{"a":"wrong"}"#).is_err());
    let ok: JsonV<Payload> = serde_json::from_str(r#"{"a":3}"#).unwrap();
    assert_eq!(ok, JsonV(Payload { a: 3 }));
}

#[test]
fn deserialize_maybe_str_error_paths() {
    #[derive(Debug, serde::Deserialize)]
    struct AsPayload {
        #[serde(deserialize_with = "rbdc::types::deserialize_maybe_str")]
        #[allow(dead_code)]
        inner: Payload,
    }

    // A string column containing invalid JSON.
    assert!(
        serde_json::from_value::<AsPayload>(serde_json::json!({"inner": "not json"})).is_err()
    );
    // A non-string column that cannot be parsed as the target type.
    assert!(serde_json::from_value::<AsPayload>(serde_json::json!({"inner": 1})).is_err());

    #[derive(Debug, serde::Deserialize)]
    struct AsString {
        #[serde(deserialize_with = "rbdc::types::deserialize_maybe_str")]
        inner: Option<String>,
    }
    let value: AsString = serde_json::from_value(serde_json::json!({"inner": "plain"})).unwrap();
    assert_eq!(value.inner.as_deref(), Some("plain"));
    let value: AsString = serde_json::from_value(serde_json::json!({"inner": null})).unwrap();
    assert_eq!(value.inner, None);
}
