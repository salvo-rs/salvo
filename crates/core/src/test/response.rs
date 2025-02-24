use std::borrow::Cow;
use std::io::{self, Result as IoResult, Write};

use bytes::{Bytes, BytesMut};
use encoding_rs::{Encoding, UTF_8};
use flate2::write::{GzDecoder, ZlibDecoder};
use http_body_util::BodyExt;
use mime::Mime;
use serde::de::DeserializeOwned;
use tokio::io::{Error as IoError, ErrorKind};
use zstd::stream::write::Decoder as ZstdDecoder;

use crate::catcher::status_error_bytes;
use crate::http::header::{self, CONTENT_ENCODING};
use crate::http::response::{ResBody, Response};
use crate::Error;

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
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

/// More utils functions for [`Response`].
pub trait ResponseExt {
    /// Take body as `String` from response.
    fn take_string(&mut self) -> impl Future<Output = crate::Result<String>> + Send;
    /// Take body as deserialize it to type `T` instance.
    fn take_json<T: DeserializeOwned>(&mut self) -> impl Future<Output = crate::Result<T>> + Send;
    /// Take body as `String` from response with charset.
    fn take_string_with_charset(
        &mut self,
        content_type: Option<&Mime>,
        charset: &str,
        compress: Option<&str>,
    ) -> impl Future<Output = crate::Result<String>>;
    /// Take all body bytes. If body is none, it will creates and returns a new [`Bytes`].
    fn take_bytes(&mut self, content_type: Option<&Mime>) -> impl Future<Output = crate::Result<Bytes>> + Send;
}

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
        self.take_string_with_charset(content_type.as_ref(), charset, encoding.as_deref())
            .await
    }
    async fn take_json<T: DeserializeOwned>(&mut self) -> crate::Result<T> {
        let full = self.take_bytes(Some(&mime::APPLICATION_JSON)).await?;
        serde_json::from_slice(&full).map_err(Error::SerdeJson)
    }
    async fn take_string_with_charset(
        &mut self,
        content_type: Option<&Mime>,
        charset: &str,
        compress: Option<&str>,
    ) -> crate::Result<String> {
        let charset = Encoding::for_label(charset.as_bytes()).unwrap_or(UTF_8);
        let mut full = self.take_bytes(content_type).await?;
        if let Some(algo) = compress {
            match algo {
                "gzip" => {
                    let mut decoder = GzDecoder::new(Writer::new());
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
                    let mut decoder = ZstdDecoder::new(Writer::new()).expect("failed to create zstd decoder");
                    decoder.write_all(full.as_ref())?;
                    decoder.flush()?;
                    full = decoder.get_mut().take();
                }
                _ => {
                    tracing::error!(algo, "unknown compress format");
                }
            }
        }
        let (text, _, _) = charset.decode(&full);
        if let Cow::Owned(s) = text {
            return Ok(s);
        }
        String::from_utf8(full.to_vec()).map_err(|e| IoError::new(ErrorKind::Other, e).into())
    }
    async fn take_bytes(&mut self, content_type: Option<&Mime>) -> crate::Result<Bytes> {
        let body = self.take_body();
        let bytes = match body {
            ResBody::None => Bytes::new(),
            ResBody::Once(bytes) => bytes,
            ResBody::Error(e) => {
                if let Some(content_type) = content_type {
                    status_error_bytes(&e, content_type, None).1
                } else {
                    status_error_bytes(&e, &mime::TEXT_HTML, None).1
                }
            }
            _ => BodyExt::collect(body).await?.to_bytes(),
        };
        Ok(bytes)
    }
}
