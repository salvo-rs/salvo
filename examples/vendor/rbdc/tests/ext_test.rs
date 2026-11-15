//! Tests for the `try_stream!` macro / TryAsyncStream.

use futures_util::StreamExt;

#[tokio::test]
async fn test_try_stream_yields_values() {
    let stream = rbdc::try_stream! {
        r#yield!(1i32);
        r#yield!(2i32);
        Ok(())
    };
    let items: Vec<Result<i32, rbdc::Error>> = stream.collect().await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_ref().unwrap(), &1);
    assert_eq!(items[1].as_ref().unwrap(), &2);
}

#[tokio::test]
async fn test_try_stream_propagates_error() {
    let stream = rbdc::try_stream! {
        r#yield!(1i32);
        Err::<(), _>(rbdc::Error::from("boom"))
    };
    let items: Vec<Result<i32, rbdc::Error>> = stream.collect().await;
    assert_eq!(items.len(), 2);
    assert!(items[0].is_ok());
    assert!(items[1].is_err());
    assert_eq!(items[1].as_ref().unwrap_err().to_string(), "boom");
}
