//! Fragment boundaries must not affect uploaded bytes or cleanup ownership.
#![cfg(feature = "test")]

use bytes::Bytes;
use http_body_util::StreamBody;
use salvo_core::http::body::{Frame, ReqBody};
use salvo_core::test::TestClient;
use salvo_core::{BoxedError, Request};

fn chunks(payload: &Bytes, chunk_size: usize) -> Vec<Bytes> {
    let mut chunks = vec![Bytes::from_static(
        b"--buffered\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file.bin\"\r\n\r\n",
    )];
    for start in (0..payload.len()).step_by(chunk_size) {
        chunks.push(payload.slice(start..(start + chunk_size).min(payload.len())));
    }
    chunks.push(Bytes::from_static(b"\r\n--buffered--\r\n"));
    chunks
}

fn request(chunks: Vec<Bytes>, max_size: usize) -> Request {
    let frames = chunks
        .into_iter()
        .map(|bytes| Ok::<_, BoxedError>(Frame::data(bytes)));
    let body = ReqBody::Boxed {
        inner: Box::pin(StreamBody::new(futures_util::stream::iter(frames))),
        fuse_config: None,
    };
    let mut request = TestClient::post("http://localhost/")
        .add_header(
            "content-type",
            "multipart/form-data; boundary=buffered",
            true,
        )
        .body(body)
        .build();
    request.set_secure_max_size(max_size);
    request
}

#[tokio::test]
async fn mixed_small_and_large_fragments_preserve_all_uploaded_bytes() {
    let payload = Bytes::from(
        (0..(3 * 65536 + 37))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>(),
    );
    for chunk_size in [1, 31, 65535, 65536, 65537, 100000] {
        let mut req = request(chunks(&payload, chunk_size), payload.len() + 1024);
        let form = req.form_data().await.unwrap();
        let file = form.files.get("file").unwrap();
        assert_eq!(file.size(), payload.len() as u64);
        assert_eq!(
            tokio::fs::read(file.path()).await.unwrap(),
            payload.as_ref(),
            "chunk_size={chunk_size}"
        );
    }
    let mut empty = request(chunks(&Bytes::new(), 1), 1024);
    assert_eq!(
        empty
            .form_data()
            .await
            .unwrap()
            .files
            .get("file")
            .unwrap()
            .size(),
        0
    );
}

#[tokio::test]
#[ignore = "manual release-mode fragmentation performance comparison"]
async fn fragmentation_parse_medians() {
    let payload = Bytes::from(vec![b'x'; 16384]);
    for chunk_size in [16384, 4096, 64, 1] {
        let prepared = chunks(&payload, chunk_size);
        let mut samples = Vec::new();
        for sample in 0..8 {
            let mut req = request(prepared.clone(), payload.len() + 1024);
            let started = std::time::Instant::now();
            let form = req.form_data().await.unwrap();
            let elapsed = started.elapsed();
            assert_eq!(form.files.get("file").unwrap().size(), payload.len() as u64);
            if sample != 0 {
                samples.push(elapsed.as_nanos());
            }
        }
        samples.sort_unstable();
        println!(
            "multipart_fragmentation,bytes={},input_chunk={},samples={},median_ns={}",
            payload.len(),
            chunk_size,
            samples.len(),
            samples[samples.len() / 2]
        );
    }
}
