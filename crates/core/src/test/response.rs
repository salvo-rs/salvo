use std::borrow::Cow;
use std::io::{self, Write};

use zstd::stream::write::Decoder as ZstdDecoder;
use flate2::write::{GzDecoder, ZlibDecoder};
use bytes::{Bytes, BytesMut};
use encoding_rs::{Encoding, UTF_8};
use futures_util::stream::StreamExt;
use http_body_util::BodyExt;
use mime::Mime;
use serde::de::DeserializeOwned;
use tokio::io::{Error as IoError, ErrorKind};

use crate::http::header::{self, CONTENT_ENCODING};
use crate::http::response::{ResBody, Response};
use crate::{async_trait, Error};

struct Writer {
    buf: BytesMut,
}

impl Writer {
    fn new() -> Writer {
        Writer {
            buf: BytesMut::with_capacity(8192),
        }
    }

    fn take(&mut self) -> Bytes {
        self.buf.split().freeze()
    }
}

impl io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// More utils functions for response.
#[async_trait]
pub trait ResponseExt {
    /// Take body as `String` from response.
    async fn take_string(&mut self) -> crate::Result<String>;
    /// Take body as deserialize it to type `T` instance.
    async fn take_json<T: DeserializeOwned>(&mut self) -> crate::Result<T>;
    /// Take body as `String` from response with charset.
    async fn take_string_with_charset(&mut self, charset: &str, compress: Option<&str>) -> crate::Result<String>;
    /// Take all body bytes. If body is none, it will creates and returns a new [`Bytes`].
    async fn take_bytes(&mut self) -> crate::Result<Bytes>;
}

#[async_trait]
impl ResponseExt for Response {
    async fn take_string(&mut self) -> crate::Result<String> {
        let content_type = self
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<Mime>().ok());
        let charset = content_type
            .as_ref()
            .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
            .unwrap_or("utf-8");
        let encoding = self
            .headers()
            .get(CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        self.take_string_with_charset(charset, encoding.as_deref()).await
    }
    async fn take_json<T: DeserializeOwned>(&mut self) -> crate::Result<T> {
        let full = self.take_bytes().await?;
        serde_json::from_slice(&full).map_err(Error::SerdeJson)
    }
    async fn take_string_with_charset(&mut self, charset: &str, compress: Option<&str>) -> crate::Result<String> {
        let charset = Encoding::for_label(charset.as_bytes()).unwrap_or(UTF_8);
        let mut full = self.take_bytes().await?;
        if let Some(algo) = compress {
            match algo {
                "gzip" => {
                    let mut decoder = GzDecoder::new(
                        Writer::new(),
                    );
                    decoder.write_all(full.as_ref())?;
                    decoder.flush()?;
                    full = decoder.get_mut().take();
                }
                "deflate" => {
                    let mut decoder = ZlibDecoder::new(Writer::new());
                    decoder.write_all(full.as_ref())?;
                    decoder.flush()?;
                    full = decoder.get_mut().take();
                }
                "br" => {
                    let mut decoder = brotli::DecompressorWriter::new(Writer::new(), 8_096);
                    decoder.write_all(full.as_ref())?;
                    decoder.flush()?;
                    full = decoder.get_mut().take();
                }
                "zstd" => {
                    let mut decoder = ZstdDecoder::new(Writer::new()).expect(
                        "failed to create zstd decoder"
                    );
                    decoder.write_all(full.as_ref())?;
                    decoder.flush()?;
                    full = decoder.get_mut().take();
                }
                _ => {
                    tracing::error!(compress = %algo, "unknown compress format");
                }
            }
        }
        let (text, _, _) = charset.decode(&full);
        if let Cow::Owned(s) = text {
            return Ok(s);
        }
        String::from_utf8(full.to_vec()).map_err(|e| IoError::new(ErrorKind::Other, e).into())
    }
    async fn take_bytes(&mut self) -> crate::Result<Bytes> {
        let body = self.take_body();
        let bytes = match body {
            ResBody::None => Bytes::new(),
            ResBody::Once(bytes) => bytes,
            ResBody::Chunks(chunks) => {
                let mut bytes = BytesMut::new();
                for chunk in chunks {
                    bytes.extend(chunk);
                }
                bytes.freeze()
            }
            ResBody::Hyper(body) => body.collect().await?.to_bytes(),
            ResBody::Stream(mut stream) => {
                let mut bytes = BytesMut::new();
                while let Some(chunk) = stream.next().await {
                    bytes.extend(chunk?);
                }
                bytes.freeze()
            }
        };
        Ok(bytes)
    }
}
