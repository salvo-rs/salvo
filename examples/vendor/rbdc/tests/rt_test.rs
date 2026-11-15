//! Tests for `rbdc::rt`.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_block_on() {
    let value = rbdc::rt::block_on(async { 1 + 1 });
    assert_eq!(value, 2);
}
