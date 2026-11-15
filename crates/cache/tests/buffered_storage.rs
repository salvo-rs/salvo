use std::collections::VecDeque;

use bytes::Bytes;
use salvo_cache::CachedBody;
use salvo_core::http::ResBody;

#[test]
fn one_large_chunk_keeps_shared_storage() {
    let bytes = Bytes::from(vec![9; 1024 * 1024]);
    let response = ResBody::Chunks(VecDeque::from([bytes.clone()]));
    let CachedBody::Once(cached) = CachedBody::try_from(&response).unwrap() else {
        panic!("single chunk should remain shared bytes")
    };
    assert_eq!(cached.as_ptr(), bytes.as_ptr());
    assert_eq!(cached.len(), bytes.len());
}

#[test]
fn compaction_is_bounded_and_preserves_large_chunk_bytes() {
    let bytes = Bytes::from(vec![7; 128 * 1024]);
    let chunks = VecDeque::from([bytes.slice(..65536), bytes.slice(65536..)]);
    let CachedBody::Chunks(cached) = CachedBody::try_from(&ResBody::Chunks(chunks)).unwrap() else {
        panic!("large response must not require a contiguous copy")
    };
    assert_eq!(cached[0].as_ptr(), bytes.as_ptr());
    assert_eq!(cached.iter().map(Bytes::len).sum::<usize>(), bytes.len());
}

#[test]
fn many_small_chunks_are_stored_once_without_changing_the_payload() {
    let response = ResBody::Chunks(VecDeque::from([
        Bytes::from_static(b"ab"),
        Bytes::new(),
        Bytes::from_static(b"cd"),
    ]));
    let CachedBody::Once(cached) = CachedBody::try_from(&response).unwrap() else {
        panic!("small fragmented response should compact")
    };
    assert_eq!(cached, "abcd");
}
