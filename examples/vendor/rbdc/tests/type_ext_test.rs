//! Tests for the `rbdc::types` wrappers (Date, Time, DateTime, Timestamp,
//! Uuid, Bytes, BytesSize, Json, Decimal).

use bigdecimal::ToPrimitive;
use rbdc::types::decimal::RoundingMode;
use rbdc::{
    Bytes, BytesSize, Date, DateTime, Decimal, Json, Time, Timestamp, Uuid, B, EB, GB, KB, MB, TB,
};
use rbs::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

// ---------------------------------------------------------------------------
// Date
// ---------------------------------------------------------------------------

#[test]
fn test_date() {
    let d = Date::default();
    assert_eq!(d.to_string(), "1970-01-01");
    assert_eq!(format!("{:?}", d), "Date(1970-01-01)");

    let d2 = Date::from_str("2024-01-15").unwrap();
    assert_eq!(d2.to_string(), "2024-01-15");
    assert!("not-a-date".parse::<Date>().is_err());

    let value: Value = d2.clone().into();
    assert_eq!(
        value,
        Value::Ext("Date", Box::new(Value::String("2024-01-15".into())))
    );

    let fast: fastdate::Date = d2.clone().into();
    assert_eq!(fast.to_string(), "2024-01-15");

    let dt = DateTime::from_str("2024-01-15T00:00:00Z").unwrap();
    assert_eq!(
        Date::from(fastdate::DateTime::from(dt)).to_string(),
        "2024-01-15"
    );

    let json = serde_json::to_string(&d2).unwrap();
    let back: Date = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d2);
    assert_eq!(hash_of(&back), hash_of(&d2));
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

#[test]
fn test_time() {
    let t = Time::default();
    assert_eq!(t.to_string(), "00:00:00");
    assert_eq!(format!("{:?}", t), "Time(00:00:00)");

    let t2 = Time::from_str("12:34:56").unwrap();
    assert_eq!(t2.to_string(), "12:34:56");
    assert!("nope".parse::<Time>().is_err());

    let value: Value = t2.clone().into();
    assert_eq!(
        value,
        Value::Ext("Time", Box::new(Value::String("12:34:56".into())))
    );

    let fast: fastdate::Time = t2.clone().into();
    assert_eq!(fast.to_string(), "12:34:56");

    let dt = DateTime::from_str("2024-01-15T12:34:56Z").unwrap();
    assert_eq!(Time::from(dt).to_string(), "12:34:56");

    let json = serde_json::to_string(&t2).unwrap();
    let back: Time = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t2);
}

// ---------------------------------------------------------------------------
// Timestamp
// ---------------------------------------------------------------------------

#[test]
fn test_timestamp() {
    let ts = Timestamp(123);
    assert_eq!(ts.to_string(), "123");
    assert_eq!(format!("{:?}", ts), "Timestamp(123)");
    assert_eq!(Timestamp::default(), Timestamp(0));

    assert_eq!(
        Value::from(ts.clone()),
        Value::Ext("Timestamp", Box::new(Value::I64(123)))
    );

    assert_eq!(Timestamp::from_str("456").unwrap(), Timestamp(456));
    assert!("abc".parse::<Timestamp>().is_err());

    let dt = DateTime::from_timestamp_millis(1000);
    assert_eq!(Timestamp::from(dt.clone()), Timestamp(1000));
    let back: DateTime = Timestamp(1000).into();
    assert_eq!(back, dt);

    let json = serde_json::to_string(&ts).unwrap();
    let back: Timestamp = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ts);
    // A non-integer payload is rejected.
    assert!(serde_json::from_value::<Timestamp>(serde_json::json!("nope")).is_err());
    // Markers used by drivers (always UTC so tests are timezone-independent).
    let _ = Timestamp::utc();
}

// ---------------------------------------------------------------------------
// Uuid
// ---------------------------------------------------------------------------

#[test]
fn test_uuid() {
    let u = Uuid::default();
    assert_eq!(u.to_string(), "00000000-0000-0000-0000-000000000000");
    assert_eq!(
        format!("{:?}", u),
        "Uuid(00000000-0000-0000-0000-000000000000)"
    );

    let generated = Uuid::new();
    assert_eq!(generated.0.len(), 36);
    assert!(uuid::Uuid::parse_str(&generated.0).is_ok());

    let parsed = Uuid::from_str("abc").unwrap();
    assert_eq!(parsed.0, "abc");

    let value: Value = parsed.clone().into();
    assert_eq!(
        value,
        Value::Ext("Uuid", Box::new(Value::String("abc".into())))
    );

    let json = serde_json::to_string(&parsed).unwrap();
    let back: Uuid = serde_json::from_str(&json).unwrap();
    assert_eq!(back, parsed);
    assert!(serde_json::from_value::<Uuid>(serde_json::json!(1)).is_err());
}

// ---------------------------------------------------------------------------
// Bytes
// ---------------------------------------------------------------------------

#[test]
fn test_bytes() {
    let empty = Bytes::new();
    assert!(empty.as_ref().is_empty());
    assert_eq!(empty, Bytes::default());

    let reserve = Bytes::with_capacity(16);
    assert!(reserve.as_ref().is_empty());

    let b = Bytes::from(vec![b'a', b'b', b'c']);
    assert_eq!(b, b"abc".as_slice());
    assert_eq!(format!("{:?}", b), "[97, 98, 99]");
    assert!(b < Bytes::from(b"abd".to_vec()));
    assert!(b > Bytes::from(b"abb".to_vec()));

    let from_vec = Bytes::from(vec![1u8, 2, 3]);
    assert_eq!(from_vec.as_ref(), &[1, 2, 3]);
    assert_eq!(from_vec.clone().into_inner(), vec![1, 2, 3]);

    let slice: &[u8] = &[4u8, 5];
    let from_slice: Bytes = slice.into();
    assert_eq!(from_slice.as_ref(), &[4, 5]);

    assert_eq!(hash_of(&from_vec), hash_of(&vec![1u8, 2, 3]));

    // IntoIterator for owned, & and &mut.
    let owned: Vec<u8> = from_vec.clone().into_iter().collect();
    assert_eq!(owned, vec![1, 2, 3]);
    let mut sum = 0u8;
    for v in &from_vec {
        sum += *v;
    }
    assert_eq!(sum, 6);
    let mut mut_bytes = from_vec.clone();
    for v in &mut mut_bytes {
        *v += 1;
    }
    assert_eq!(mut_bytes.as_ref(), &[2, 3, 4]);

    let json = serde_json::to_string(&from_vec).unwrap();
    assert_eq!(json, "[1,2,3]");
    let back: Bytes = serde_json::from_str(&json).unwrap();
    assert_eq!(back, from_vec);
}

#[test]
fn test_bytes_size() {
    assert_eq!(B.into_inner(), 1);
    assert_eq!(KB.into_inner(), 1024);
    assert_eq!(MB.into_inner(), 1024 * 1024);
    assert_eq!(GB.into_inner(), 1024 * 1024 * 1024);
    assert_eq!(TB.into_inner(), 1024i64.pow(4));
    assert_eq!(EB.into_inner(), 1024i64.pow(5));

    assert_eq!(BytesSize::default(), BytesSize(0));
    assert_eq!(BytesSize::from(10), BytesSize(10));

    assert_eq!(BytesSize(0).display(), "0.00B");
    assert_eq!(format!("{}", BytesSize(512)), "512.00B");
    assert_eq!(format!("{}", KB), "1.00KB");
    assert_eq!(format!("{}", MB), "1.00MB");
    assert_eq!(format!("{}", GB), "1.00GB");
    assert_eq!(format!("{}", TB), "1.00TB");
    assert_eq!(format!("{}", EB), "1.00EB");
    assert_eq!(format!("{}", BytesSize(-1024)), "-1024.00B");

    assert_eq!(BytesSize::from_str("10B").unwrap(), BytesSize(10));
    assert_eq!(BytesSize::from_str("10").unwrap(), BytesSize(10));
    assert_eq!(BytesSize::from_str("1.5KB").unwrap(), BytesSize(1536));
    assert_eq!(
        BytesSize::from_str("2MB").unwrap(),
        BytesSize(2 * 1024 * 1024)
    );
    assert_eq!(
        BytesSize::from_str("1GB").unwrap(),
        BytesSize(1024 * 1024 * 1024)
    );
    assert_eq!(BytesSize::from_str("1TB").unwrap(), TB);
    assert_eq!(BytesSize::from_str("1EB").unwrap(), EB);
    assert!(BytesSize::from_str("abc").is_err());

    let json = serde_json::to_string(&BytesSize(42)).unwrap();
    assert_eq!(json, "42");
    let back: BytesSize = serde_json::from_str(&json).unwrap();
    assert_eq!(back, BytesSize(42));

    assert!(KB > B);
}

// ---------------------------------------------------------------------------
// Json
// ---------------------------------------------------------------------------

#[test]
fn test_json_from_rbs_value() {
    assert_eq!(Json::default().0, "null");
    assert_eq!(Json::from(Value::Null).0, "null");
    assert_eq!(Json::from(Value::Bool(true)).0, "true");
    assert_eq!(Json::from(Value::I32(1)).0, "1");
    assert_eq!(Json::from(Value::I64(2)).0, "2");
    assert_eq!(Json::from(Value::U32(3)).0, "3");
    assert_eq!(Json::from(Value::U64(4)).0, "4");
    assert_eq!(Json::from(Value::F32(1.5)).0.parse::<f64>().unwrap(), 1.5);
    assert_eq!(Json::from(Value::F64(2.5)).0.parse::<f64>().unwrap(), 2.5);
    assert_eq!(Json::from(Value::Binary(vec![b'h', b'i'])).0, "hi");

    // A plain string gets quoted, JSON-looking strings are kept as-is.
    assert_eq!(Json::from(Value::String("abc".into())).0, "\"abc\"");
    assert_eq!(
        Json::from(Value::String("{\"a\":1}".into())).0,
        "{\"a\":1}"
    );
    assert_eq!(
        Json::from(Value::String("[1,2]".into())).0,
        "[1,2]"
    );
    assert_eq!(Json::from(Value::String("\"x\"".into())).0, "\"x\"");

    let arr = Value::Array(vec![Value::I64(1), Value::I64(2)]);
    assert_eq!(Json::from(arr).0, "[1,2]");
    let mut map = rbs::value::map::ValueMap::new();
    map.insert("a".into(), 1.into());
    assert_eq!(Json::from(Value::Map(map)).0, "{\"a\":1}");

    let ext = Value::Ext("x", Box::new(Value::I64(7)));
    assert_eq!(Json::from(ext).0, "7");
}

#[test]
fn test_json_misc() {
    let json = Json::from(serde_json::json!({"a": 1}));
    assert_eq!(json.0, "{\"a\":1}");
    assert_eq!(json.to_string(), "{\"a\":1}");
    assert_eq!(format!("{:?}", json), "Json({\"a\":1})");

    let value: Value = json.clone().into();
    assert_eq!(
        value,
        Value::Ext("Json", Box::new(Value::String("{\"a\":1}".into())))
    );

    let serde_value: serde_json::Value = json.clone().into();
    assert_eq!(serde_value, serde_json::json!({"a": 1}));
    let invalid: serde_json::Value = Json("not json".into()).into();
    assert!(invalid.is_null());

    assert_eq!(Json::from_str("{\"b\":2}").unwrap().0, "{\"b\":2}");
    let decoded: Json = serde_json::from_str("{\"b\":2}").unwrap();
    assert_eq!(decoded.0, "{\"b\":2}");
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Payload {
    a: i32,
    b: String,
}

#[test]
fn test_json_v() {
    use rbdc::JsonV;

    let payload = Payload {
        a: 1,
        b: "x".into(),
    };
    let jv = JsonV(payload);

    // serde_json path serializes the inner value directly.
    let json = serde_json::to_string(&jv).unwrap();
    assert_eq!(json, r#"{"a":1,"b":"x"}"#);
    let back: JsonV<Payload> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, jv);

    // rbs path encodes the value as a Json extension.
    let value = rbs::value!(&jv);
    let back: JsonV<Payload> = rbs::from_value(value).unwrap();
    assert_eq!(back, jv);

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Wrapper {
        #[serde(deserialize_with = "rbdc::types::deserialize_maybe_str")]
        inner: Payload,
    }

    let from_string: Wrapper =
        serde_json::from_value(serde_json::json!({"inner": "{\"a\":2,\"b\":\"y\"}"})).unwrap();
    assert_eq!(from_string.inner, Payload { a: 2, b: "y".into() });
    let from_object: Wrapper =
        serde_json::from_value(serde_json::json!({"inner": {"a": 3, "b": "z"}})).unwrap();
    assert_eq!(from_object.inner, Payload { a: 3, b: "z".into() });

    let as_string: String =
        serde_json::from_value(serde_json::json!("plain")).expect("string target");
    assert_eq!(as_string, "plain");
}

// ---------------------------------------------------------------------------
// Decimal
// ---------------------------------------------------------------------------

#[test]
fn test_decimal_constructors() {
    assert_eq!(Decimal::new("1.5").unwrap().to_string(), "1.5");
    assert!(Decimal::new("nope").is_err());

    assert_eq!(Decimal::from_f64(1.5).unwrap().to_string(), "1.5");
    assert_eq!(Decimal::from_f32(1.5).unwrap().to_string(), "1.5");
    assert!(Decimal::from_f64(f64::NAN).is_none());
    assert!(Decimal::from_f32(f32::NAN).is_none());

    assert_eq!(Decimal::default().to_string(), "0");
    assert_eq!(Decimal::from(1i32).to_string(), "1");
    assert_eq!(Decimal::from(1u32).to_string(), "1");
    assert_eq!(Decimal::from(1i64).to_string(), "1");
    assert_eq!(Decimal::from(1u64).to_string(), "1");
    assert_eq!(Decimal::from(1i128).to_string(), "1");
    assert_eq!(Decimal::from(1u128).to_string(), "1");
    assert_eq!(Decimal::try_from(1.5f64).unwrap().to_string(), "1.5");
    assert_eq!(Decimal::try_from(1.5f32).unwrap().to_string(), "1.5");
}

#[test]
fn test_decimal_ops() {
    let one = Decimal::new("1").unwrap();
    let two = Decimal::new("2").unwrap();

    assert_eq!((one.clone() + two.clone()).to_string(), "3");
    assert_eq!((one.clone() - two.clone()).to_string(), "-1");
    assert_eq!((one.clone() * two.clone()).to_string(), "2");
    assert_eq!((two.clone() / one.clone()).to_string(), "2");
    assert_eq!((Decimal::new("7").unwrap() % two.clone()).to_string(), "1");
    assert_eq!((-one.clone()).to_string(), "-1");

    let mut v = one.clone();
    v += two.clone();
    assert_eq!(v.to_string(), "3");
    v -= two.clone();
    assert_eq!(v.to_string(), "1");
    v *= two.clone();
    assert_eq!(v.to_string(), "2");

    assert!(one < two);
    assert!(two > one);
    assert_eq!(one.partial_cmp(&two), Some(std::cmp::Ordering::Less));

    assert_eq!(Decimal::new("-1.5").unwrap().abs().to_string(), "1.5");
    assert_eq!(Decimal::new("1.23456").unwrap().with_scale(2).to_string(), "1.23");
    assert_eq!(Decimal::new("1.23456").unwrap().with_prec(2).to_string(), "1.2");
    assert_eq!(Decimal::new("1.5").unwrap().round(0).to_string(), "2");
    assert_eq!(
        Decimal::new("1.23456")
            .unwrap()
            .with_scale_round(2, RoundingMode::Up)
            .to_string(),
        "1.24"
    );
    assert_eq!(Decimal::new("123.45").unwrap().digits(), 5);
    assert_eq!(
        Decimal::new("1.23456").unwrap().fractional_digit_count(),
        5
    );
}

#[test]
fn test_decimal_conversions() {
    let d = Decimal::new("42.9").unwrap();
    assert_eq!(d.to_i64(), Some(42));
    assert_eq!(d.to_i128(), Some(42));
    assert_eq!(d.to_u64(), Some(42));
    assert_eq!(d.to_u128(), Some(42));
    assert!((d.to_f64().unwrap() - 42.9).abs() < 1e-9);

    let value: Value = d.clone().into();
    assert_eq!(
        value,
        Value::Ext("Decimal", Box::new(Value::String("42.9".into())))
    );

    assert_eq!(format!("{}", d), "42.9");
    assert_eq!(format!("{:?}", d), "Decimal(42.9)");

    let json = serde_json::to_string(&d).unwrap();
    let back: Decimal = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);

    // Deserialization accepts numbers and extension values.
    let from_number: Decimal = serde_json::from_value(serde_json::json!(3)).unwrap();
    assert_eq!(from_number, Decimal::from(3));
    let from_ext: Decimal =
        rbs::from_value(Value::Ext("Decimal", Box::new(Value::String("1.25".into())))).unwrap();
    assert_eq!(from_ext, Decimal::new("1.25").unwrap());
    assert!(rbs::from_value::<Decimal>(Value::String("bad".into())).is_err());
}

// ---------------------------------------------------------------------------
// DateTime
// ---------------------------------------------------------------------------

#[test]
fn test_datetime_construction_and_parts() {
    let dt = DateTime::from_str("2024-01-15T12:34:56Z").unwrap();
    assert_eq!(dt.to_string(), "2024-01-15T12:34:56Z");
    assert_eq!(format!("{:?}", dt), "DateTime(2024-01-15T12:34:56Z)");
    assert_eq!(dt.year(), 2024);
    assert_eq!(dt.mon(), 1);
    assert_eq!(dt.day(), 15);
    assert_eq!(dt.hour(), 12);
    assert_eq!(dt.minute(), 34);
    assert_eq!(dt.sec(), 56);
    assert_eq!(dt.nano(), 0);
    assert_eq!(dt.ms(), 0);
    assert_eq!(dt.micro(), 0);
    assert_eq!(dt.offset(), 0);
    assert!(dt.week_day() <= 6);

    let default = DateTime::default();
    assert_eq!(default.to_string(), "1970-01-01T00:00:00Z");
    assert_eq!(default, DateTime::from_timestamp(0));

    assert_eq!(DateTime::from_timestamp_micros(0), default);
    assert_eq!(DateTime::from_timestamp_millis(0), default);
    assert_eq!(DateTime::from_timestamp_nano(0), default);
    assert_eq!(DateTime::from_system_time(UNIX_EPOCH, 0), default);

    assert_eq!(default.unix_timestamp(), 0);
    assert_eq!(default.unix_timestamp_micros(), 0);
    assert_eq!(default.unix_timestamp_millis(), 0);
    assert_eq!(default.unix_timestamp_nano(), 0);

    let later = DateTime::from_timestamp(60);
    assert!(default.before(&later));
    assert!(later.after(&default));

    // Offset / formatting helpers.
    let shifted = dt.clone().set_offset(8 * 3600);
    assert_eq!(shifted.offset(), 8 * 3600);
    assert_eq!(dt.format("YYYY-MM-DD/hh/mm/ss"), "2024-01-15/12/34/56");
    let parsed = DateTime::parse("YYYY-MM-DD hh:mm:ss", "2024-01-15 12:34:56").unwrap();
    assert_eq!(parsed.format("YYYY-MM-DD hh:mm:ss"), dt.format("YYYY-MM-DD hh:mm:ss"));
    assert!(DateTime::parse("YYYY-MM-DD", "bad").is_err());

    assert!(dt.display_stand().starts_with("2024-01-15 12:34:56"));
    assert!(dt.display(true).starts_with("2024-01-15T12:34:56"));
    assert!(dt.display(false).starts_with("2024-01-15T12:34:56"));

    let mut buf: [u8; 38] = *b"0000-00-00T00:00:00.000000000+00:00:00";
    let n = dt.do_display(&mut buf, true);
    assert!(std::str::from_utf8(&buf[..n]).unwrap().starts_with("2024-01-15T12:34:56"));

    // fastdate's `set_nano` adds the argument as microseconds; verify the
    // method runs and the resulting nanosecond field reflects that.
    let with_nano = dt.clone().set_nano(123);
    assert_eq!(with_nano.nano(), 123_000);
}

#[test]
fn test_datetime_arithmetic() {
    let base = DateTime::from_timestamp(100);

    assert_eq!(base.clone().add(Duration::from_secs(10)), DateTime::from_timestamp(110));
    assert_eq!(base.clone().sub(Duration::from_secs(10)), DateTime::from_timestamp(90));
    assert_eq!(
        base.clone() + &Duration::from_secs(10),
        DateTime::from_timestamp(110)
    );
    assert_eq!(
        base.clone() - &Duration::from_secs(10),
        DateTime::from_timestamp(90)
    );
    assert_eq!(base.clone().add_sub_sec(5), DateTime::from_timestamp(105));

    let diff = DateTime::from_timestamp(160) - base.clone();
    assert_eq!(diff, Duration::from_secs(60));

    assert!(base < DateTime::from_timestamp(101));
    assert!(base.clone().cmp(&DateTime::from_timestamp(100)) == std::cmp::Ordering::Equal);
}

#[test]
fn test_datetime_conversions() {
    let dt = DateTime::from_str("2024-01-15T12:34:56Z").unwrap();

    let system: SystemTime = dt.clone().into();
    let back: DateTime = system.into();
    assert_eq!(back, dt);

    let before_epoch = DateTime::from_timestamp(-1);
    let system: SystemTime = before_epoch.clone().into();
    assert_eq!(system, UNIX_EPOCH - Duration::from_secs(1));
    assert!(system < UNIX_EPOCH);

    assert_eq!(
        DateTime::from(Date::from_str("2024-01-15").unwrap()).to_string(),
        "2024-01-15T00:00:00Z"
    );
    assert_eq!(
        DateTime::from(Time::from_str("12:34:56").unwrap()).to_string(),
        "0000-01-01T12:34:56Z"
    );
    assert_eq!(
        DateTime::from((
            Date::from_str("2024-01-15").unwrap(),
            Time::from_str("12:34:56").unwrap()
        ))
        .to_string(),
        "2024-01-15T12:34:56Z"
    );
    assert_eq!(
        DateTime::from((
            Date::from_str("2024-01-15").unwrap(),
            Time::from_str("12:34:56").unwrap(),
            0
        ))
        .to_string(),
        "2024-01-15T12:34:56Z"
    );

    let value: Value = dt.clone().into();
    assert_eq!(
        value,
        Value::Ext("DateTime", Box::new(Value::String(dt.to_string())))
    );
    let fast: fastdate::DateTime = dt.clone().into();
    assert_eq!(fast.to_string(), dt.to_string());

    // Serialization round trips.
    let json = serde_json::to_string(&dt).unwrap();
    let back: DateTime = serde_json::from_str(&json).unwrap();
    assert_eq!(back, dt);

    // rbs deserialization accepts ints, strings and rejects other types.
    assert_eq!(
        rbs::from_value::<DateTime>(Value::I32(0)).unwrap(),
        DateTime::default()
    );
    assert_eq!(
        rbs::from_value::<DateTime>(Value::U32(0)).unwrap(),
        DateTime::default()
    );
    assert_eq!(
        rbs::from_value::<DateTime>(Value::I64(0)).unwrap(),
        DateTime::default()
    );
    assert_eq!(
        rbs::from_value::<DateTime>(Value::U64(0)).unwrap(),
        DateTime::default()
    );
    assert_eq!(
        rbs::from_value::<DateTime>(Value::String(dt.to_string())).unwrap(),
        dt
    );
    assert!(rbs::from_value::<DateTime>(Value::Bool(true)).is_err());
    assert!("bad-date".parse::<DateTime>().is_err());

    // UTC only: `DateTime::now()` would pick up the host timezone.
    let _ = DateTime::utc();
}

// ---------------------------------------------------------------------------
// Value <-> fastdate checks
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_as_mut_and_parse_errors() {
    let mut b = Bytes::from(vec![1u8, 2, 3]);
    b.as_mut()[0] = 9;
    assert_eq!(b.as_ref(), &[9, 2, 3]);

    for s in ["xEB", "xTB", "xGB", "xMB", "xKB", "xB", "x"] {
        assert!(BytesSize::from_str(s).is_err(), "should reject {s}");
    }
}

#[test]
fn test_decimal_from_primitive() {
    use bigdecimal::FromPrimitive;

    assert_eq!(
        <Decimal as FromPrimitive>::from_i64(1),
        Some(Decimal::from(1))
    );
    assert_eq!(
        <Decimal as FromPrimitive>::from_u64(1),
        Some(Decimal::from(1))
    );
    assert_eq!(
        <Decimal as FromPrimitive>::from_i128(1),
        Some(Decimal::from(1))
    );
    assert_eq!(
        <Decimal as FromPrimitive>::from_u128(1),
        Some(Decimal::from(1))
    );
    assert_eq!(
        <Decimal as FromPrimitive>::from_f32(1.5),
        Some(Decimal::new("1.5").unwrap())
    );
    assert_eq!(
        <Decimal as FromPrimitive>::from_f64(1.5),
        Some(Decimal::new("1.5").unwrap())
    );
    assert!(Decimal::try_from(f64::NAN).is_err());

    // Deserialization from a bare number takes the `_` arm.
    let from_int: Decimal = rbs::from_value(Value::I64(3)).unwrap();
    assert_eq!(from_int, Decimal::from(3));
}

#[test]
fn test_datetime_operators_and_timestamp_conversion() {
    let base = DateTime::from_timestamp(100);
    assert_eq!(base.clone() + Duration::from_secs(1), DateTime::from_timestamp(101));
    assert_eq!(base.clone() - Duration::from_secs(1), DateTime::from_timestamp(99));

    let fast: fastdate::DateTime = Timestamp(5).into();
    assert_eq!(fast.unix_timestamp_millis(), 5);
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct StringField {
    #[serde(deserialize_with = "rbdc::types::deserialize_maybe_str")]
    value: String,
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct OptionStringField {
    #[serde(deserialize_with = "rbdc::types::deserialize_maybe_str")]
    value: Option<String>,
}

#[test]
fn test_deserialize_maybe_str_string_targets() {
    let plain: StringField =
        serde_json::from_value(serde_json::json!({"value": "hello"})).unwrap();
    assert_eq!(plain.value, "hello");

    let some: OptionStringField =
        serde_json::from_value(serde_json::json!({"value": "world"})).unwrap();
    assert_eq!(some.value.as_deref(), Some("world"));

    let none: OptionStringField =
        serde_json::from_value(serde_json::json!({"value": null})).unwrap();
    assert_eq!(none.value, None);
}

#[test]
fn test_json_v_rbs_inputs_and_display() {
    use rbdc::JsonV;
    use rbs::value::map::ValueMap;

    let from_string: JsonV<Payload> =
        rbs::from_value(Value::String("{\"a\":1,\"b\":\"x\"}".into())).unwrap();
    assert_eq!(
        from_string.0,
        Payload {
            a: 1,
            b: "x".into()
        }
    );

    let from_binary: JsonV<Payload> =
        rbs::from_value(Value::Binary(b"{\"a\":2,\"b\":\"y\"}".to_vec())).unwrap();
    assert_eq!(
        from_binary.0,
        Payload {
            a: 2,
            b: "y".into()
        }
    );

    let mut map = ValueMap::new();
    map.insert("a".into(), 3.into());
    map.insert("b".into(), "z".into());
    let from_map: JsonV<Payload> = rbs::from_value(Value::Map(map)).unwrap();
    assert_eq!(
        from_map.0,
        Payload {
            a: 3,
            b: "z".into()
        }
    );

    let ext = Value::Ext(
        "Json",
        Box::new(Value::String("{\"a\":4,\"b\":\"w\"}".into())),
    );
    let from_ext: JsonV<Payload> = rbs::from_value(ext).unwrap();
    assert_eq!(
        from_ext.0,
        Payload {
            a: 4,
            b: "w".into()
        }
    );

    assert_eq!(JsonV("hello".to_string()).to_string(), "hello");
    assert!(rbs::from_value::<JsonV<Payload>>(Value::String("not json".into())).is_err());
}

#[test]
fn test_datetime_value_ext_is_valid_json() {
    let dt = DateTime::from_timestamp(1_700_000_000);
    let value: Value = dt.clone().into();
    assert_eq!(
        value,
        Value::Ext(
            "DateTime",
            Box::new(Value::String("2023-11-14T22:13:20Z".into()))
        )
    );
    assert!(dt.to_string().contains("2023-11-14"));
}
