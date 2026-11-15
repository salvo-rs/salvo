//! Tests for the low-level I/O helpers in `rbdc::io`.

use bytes::{Bytes, BytesMut};
use rbdc::io::{BufExt, BufMutExt, BufStream, Decode, Encode, Nothing};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

/// A stream that accepts writes but always fails to flush. Used to cover the
/// error arm of `WriteAndFlush::poll`.
struct FlushFails;

impl AsyncRead for FlushFails {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for FlushFails {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "flush failed")))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[test]
fn test_buf_ext_nul() {
    let mut buf = Bytes::from_static(b"hello\0world\0");
    assert_eq!(&buf.get_bytes_nul().unwrap()[..], b"hello");
    assert_eq!(&buf.get_bytes_nul().unwrap()[..], b"world");
    // No NUL terminator left.
    assert!(buf.get_bytes_nul().is_err());
}

#[test]
fn test_buf_ext_plain_bytes() {
    let mut buf = Bytes::from_static(b"abcdef");
    assert_eq!(&buf.get_bytes(3)[..], b"abc");
    assert_eq!(&buf[..], b"def");
}

#[test]
fn test_buf_ext_str() {
    let mut buf = Bytes::from_static(b"hello\0world\0");
    assert_eq!(buf.get_str_nul().unwrap(), "hello");
    assert_eq!(buf.get_str_nul().unwrap(), "world");
    assert!(buf.get_str_nul().is_err());

    let mut buf = Bytes::from_static(b"hello world");
    assert_eq!(buf.get_str(5).unwrap(), "hello");

    let mut invalid = Bytes::from_static(&[0xff, 0xfe, b'a']);
    assert!(invalid.get_str(3).is_err());
    // The cursor must not move when decoding fails.
    assert_eq!(&invalid[..], &[0xff, 0xfe, b'a']);

    let mut invalid_nul = Bytes::from_static(&[0xff, 0x00]);
    assert!(invalid_nul.get_str_nul().is_err());
}

#[test]
fn test_buf_mut_put_str_nul() {
    let mut buf = Vec::new();
    buf.put_str_nul("abc");
    assert_eq!(buf, b"abc\0");
}

#[test]
fn test_encode_decode() {
    fn encode_default<'a>(value: &'a [u8], out: &mut Vec<u8>) {
        <&'a [u8] as Encode<'static, ()>>::encode(&value, out);
    }

    let mut out = Vec::new();
    encode_default(b"abc", &mut out);
    b"def".as_slice().encode_with(&mut out, ());
    assert_eq!(out, b"abcdef");

    let decoded = <Bytes as Decode>::decode(Bytes::from(out)).unwrap();
    assert_eq!(&decoded[..], b"abcdef");

    let _nothing = <Nothing as Decode>::decode(Bytes::new()).unwrap();
}

#[tokio::test]
async fn test_buf_stream_write_flush_read() {
    let (client, mut server) = tokio::io::duplex(1024);
    let mut stream = BufStream::new(client);

    stream.write(b"ping".as_slice());
    assert_eq!(&stream.wbuf, b"ping");
    stream.flush().await.unwrap();
    assert!(stream.wbuf.is_empty());

    let mut got = [0u8; 4];
    server.read_exact(&mut got).await.unwrap();
    assert_eq!(&got, b"ping");

    server.write_all(b"pong!!").await.unwrap();
    let value: Bytes = stream.read(6).await.unwrap();
    assert_eq!(&value[..], b"pong!!");
}

#[tokio::test]
async fn test_buf_stream_read_raw() {
    let (client, mut server) = tokio::io::duplex(64);
    let mut stream = BufStream::new(client);
    server.write_all(b"0123456789").await.unwrap();

    let raw = stream.read_raw(4).await.unwrap();
    assert_eq!(&raw[..], b"0123");

    let mut buf = BytesMut::new();
    stream.read_raw_into(&mut buf, 3).await.unwrap();
    assert_eq!(&buf[..], b"456");
}

#[tokio::test]
async fn test_buf_stream_eof() {
    let (client, server) = tokio::io::duplex(8);
    drop(server);
    let mut stream = BufStream::new(client);
    let err = stream.read_raw(3).await.unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("connection") || msg.contains("aborted"), "unexpected: {msg}");
}

#[tokio::test]
async fn test_buf_stream_deref() {
    let (client, _server) = tokio::io::duplex(8);
    let mut stream = BufStream::new(client);
    let _ = &*stream;
    let _ = &mut *stream;
}

#[tokio::test]
async fn test_write_and_flush_propagates_flush_error() {
    let mut stream = BufStream::new(FlushFails);
    stream.write(b"data".as_slice());
    let err = stream.flush().await.unwrap_err();
    assert!(err.to_string().contains("flush failed"));
    // The buffer is cleared by `Drop` even when flushing fails.
    assert!(stream.wbuf.is_empty());
}

#[tokio::test]
async fn test_write_and_flush_drop_clears_buffer() {
    let (client, _server) = tokio::io::duplex(8);
    let mut stream = BufStream::new(client);
    stream.write(b"data".as_slice());
    {
        // Dropped without being polled to completion.
        let _flush = stream.flush();
    }
    assert!(stream.wbuf.is_empty());
}
