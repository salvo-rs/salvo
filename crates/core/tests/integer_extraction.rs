#![cfg(feature = "test")]

use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use serde::Deserialize;

#[tokio::test]
async fn request_helpers_accept_full_width_integers() {
    for signed in [i128::MIN, i128::MAX, 0] {
        let mut req = TestClient::post("http://localhost/")
            .query("signed", signed)
            .add_header("signed", signed.to_string(), true)
            .raw_form(format!("signed={signed}"))
            .build();
        req.params_mut().insert("signed", signed.to_string());
        assert_eq!(req.try_param::<i128>("signed").unwrap(), signed);
        assert_eq!(req.try_query::<i128>("signed").unwrap(), signed);
        assert_eq!(req.try_header::<i128>("signed").unwrap(), signed);
        assert_eq!(req.try_form::<i128>("signed").await.unwrap(), signed);
    }
    for unsigned in [u128::MAX, u128::from(u64::MAX) + 1, 0] {
        let mut req = TestClient::post("http://localhost/")
            .query("unsigned", unsigned)
            .add_header("unsigned", unsigned.to_string(), true)
            .raw_form(format!("unsigned={unsigned}"))
            .build();
        req.params_mut().insert("unsigned", unsigned.to_string());
        assert_eq!(req.try_param::<u128>("unsigned").unwrap(), unsigned);
        assert_eq!(req.try_query::<u128>("unsigned").unwrap(), unsigned);
        assert_eq!(req.try_header::<u128>("unsigned").unwrap(), unsigned);
        assert_eq!(req.try_form::<u128>("unsigned").await.unwrap(), unsigned);
    }
}

#[derive(Debug, Deserialize, Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct WideQuery {
    signed: i128,
    unsigned: u128,
    values: Vec<i128>,
    optional: Option<u128>,
}

#[tokio::test]
async fn derived_query_accepts_scalars_lists_and_optional_integers() {
    let mut req = TestClient::get("http://localhost/")
        .query("signed", i128::MIN)
        .query("unsigned", u128::MAX)
        .query("values", format!("[{},{}]", i128::MIN, i128::MAX))
        .query("optional", u128::MAX)
        .build();
    let data: WideQuery = req.extract(&mut Depot::new()).await.unwrap();
    assert_eq!(data.signed, i128::MIN);
    assert_eq!(data.unsigned, u128::MAX);
    assert_eq!(data.values, [i128::MIN, i128::MAX]);
    assert_eq!(data.optional, Some(u128::MAX));
}

#[tokio::test]
async fn wide_integer_inputs_reject_overflow_and_invalid_numbers() {
    for value in [
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
        "1.5",
        "abc",
    ] {
        let req = TestClient::get("http://localhost/")
            .query("value", value)
            .build();
        assert!(req.try_query::<i128>("value").is_err(), "accepted {value}");
    }
    for value in [
        "340282366920938463463374607431768211456",
        "-1",
        "1.5",
        "abc",
    ] {
        let req = TestClient::get("http://localhost/")
            .query("value", value)
            .build();
        assert!(req.try_query::<u128>("value").is_err(), "accepted {value}");
    }
}
