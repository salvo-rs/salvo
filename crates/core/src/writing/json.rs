use std::collections::VecDeque;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Write};

use async_trait::async_trait;
use bytes::Bytes;
use serde::Serialize;

use super::{Scribe, try_set_header};
use crate::http::body::ResBody;
use crate::http::header::{CONTENT_TYPE, HeaderValue};
use crate::http::{Response, StatusError};

const JSON_CHUNK_SIZE: usize = 8 * 1024;

struct JsonBodyWriter {
    chunks: VecDeque<Bytes>,
    current: Vec<u8>,
}

impl JsonBodyWriter {
    fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            current: Vec::new(),
        }
    }

    fn finish(mut self) -> ResBody {
        if !self.current.is_empty() {
            self.current.shrink_to_fit();
            self.chunks.push_back(Bytes::from(self.current));
        }
        match self.chunks.len() {
            0 => ResBody::Once(Bytes::new()),
            1 => ResBody::Once(self.chunks.pop_front().expect("chunk count checked")),
            _ => ResBody::Chunks(self.chunks),
        }
    }

    fn push_current(&mut self) {
        let chunk = std::mem::take(&mut self.current);
        if !chunk.is_empty() {
            self.chunks.push_back(Bytes::from(chunk));
        }
    }
}

impl Write for JsonBodyWriter {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let len = buf.len();
        while !buf.is_empty() {
            let available = JSON_CHUNK_SIZE - self.current.len();
            if available == 0 {
                self.push_current();
                continue;
            }
            let next = available.min(buf.len());
            let target_capacity = (self.current.len() + next).min(JSON_CHUNK_SIZE);
            if self.current.capacity() < target_capacity {
                self.current.reserve(target_capacity - self.current.len());
            }
            self.current.extend_from_slice(&buf[..next]);
            buf = &buf[next..];
            if self.current.len() == JSON_CHUNK_SIZE {
                self.push_current();
            }
        }
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Writes serializable content to the response as JSON.
///
/// `Json<T>` sets `content-type` to `application/json; charset=utf-8` and
/// serializes the wrapped value with `serde_json`.
///
/// # Body semantics
///
/// A JSON body is one complete document, so rendering `Json` **replaces** any
/// body bytes previously buffered on the response (appending would produce
/// invalid JSON); debug builds log a warning when non-empty content is
/// discarded this way. Streaming bodies cannot be replaced and are reported as
/// an error, like [`Response::write_body`]. To emit multiple JSON records
/// (e.g. NDJSON), serialize each record and append it via
/// [`Response::write_body`] instead.
///
/// # Examples
///
/// ```
/// use salvo_core::prelude::*;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct User {
///     name: String,
/// }
/// #[handler]
/// async fn hello(res: &mut Response) -> Json<User> {
///     Json(User {
///         name: "jobs".into(),
///     })
/// }
/// ```
///
/// It is commonly returned together with request parsing:
///
/// ```
/// use salvo_core::http::ParseError;
/// use salvo_core::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize)]
/// struct CreateUser {
///     name: String,
/// }
///
/// #[derive(Serialize)]
/// struct User {
///     name: String,
/// }
///
/// #[handler]
/// async fn create_user(req: &mut Request) -> Result<Json<User>, ParseError> {
///     let input = req.parse_json::<CreateUser>().await?;
///     Ok(Json(User { name: input.name }))
/// }
/// ```
pub struct Json<T>(pub T);

#[async_trait]
impl<T> Scribe for Json<T>
where
    T: Serialize + Send,
{
    fn render(self, res: &mut Response) {
        let mut writer = JsonBodyWriter::new();
        match serde_json::to_writer(&mut writer, &self.0) {
            Ok(()) => {
                try_set_header(
                    &mut res.headers,
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/json; charset=utf-8"),
                );
                // A JSON body is one complete document: replace previously
                // buffered bytes instead of appending, which would concatenate
                // two documents into invalid JSON.
                match &res.body {
                    ResBody::None | ResBody::Error(_) | ResBody::Once(_) | ResBody::Chunks(_) => {
                        #[cfg(debug_assertions)]
                        {
                            let discarded = match &res.body {
                                ResBody::Once(prev) => prev.len(),
                                ResBody::Chunks(chunks) => chunks.iter().map(Bytes::len).sum(),
                                _ => 0,
                            };
                            if discarded > 0 {
                                tracing::warn!(
                                    discarded_bytes = discarded,
                                    "rendering `Json` replaced body bytes that were already \
                                     written; a JSON body is one complete document. To emit \
                                     multiple JSON records (NDJSON), serialize each record and \
                                     append it via `Response::write_body` instead."
                                );
                            }
                        }
                        res.body(writer.finish());
                    }
                    _ => {
                        // Streaming kinds cannot be replaced silently; mirror the
                        // `write_body` error behavior.
                        tracing::error!(
                            "current body kind cannot be written or replaced by `Json`"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "JSON serialize error");
                res.render(StatusError::internal_server_error());
            }
        }
    }
}
impl<T: Debug> Debug for Json<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Json").field(&self.0).finish()
    }
}
impl<T: Display> Display for Json<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    #[test]
    fn json_body_writer_does_not_preallocate_full_chunk_for_tiny_body() {
        let mut writer = JsonBodyWriter::new();

        writer.write_all(br#"{"ok":true}"#).unwrap();

        assert!(writer.current.capacity() < JSON_CHUNK_SIZE);
    }

    #[tokio::test]
    async fn test_write_json_content() {
        #[derive(Serialize, Debug)]
        struct User {
            name: String,
        }
        #[handler]
        async fn test() -> Json<User> {
            Json(User {
                name: "jobs".into(),
            })
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:8698/test")
            .send(router)
            .await;
        assert_eq!(res.take_string().await.unwrap(), r#"{"name":"jobs"}"#);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_json_render_replaces_previous_body() {
        // A JSON body is one complete document: a second render (or a render
        // after other body bytes were written) must replace, not concatenate
        // into invalid JSON.
        #[handler]
        async fn test(res: &mut Response) {
            res.render(Json(serde_json::json!({"first": true})));
            res.render(Json(serde_json::json!({"second": true})));
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:8698/test")
            .send(router)
            .await;
        let body = res.take_string().await.unwrap();
        assert_eq!(body, r#"{"second":true}"#);
        // The body must be valid JSON — the old append behavior produced two
        // concatenated documents here.
        serde_json::from_str::<serde_json::Value>(&body).expect("body must be valid JSON");
    }

    #[test]
    fn test_json_render_chunks_large_body() {
        let mut res = Response::new();
        let payload = vec!["x".repeat(JSON_CHUNK_SIZE); 2];

        Json(payload).render(&mut res);

        let ResBody::Chunks(chunks) = &res.body else {
            panic!("large JSON should be rendered as chunks");
        };
        assert!(chunks.len() > 1);
        assert!(res.body.size().unwrap() > JSON_CHUNK_SIZE as u64);
        let body = chunks
            .iter()
            .flat_map(|chunk| chunk.iter().copied())
            .collect::<Vec<_>>();
        let decoded =
            serde_json::from_slice::<Vec<String>>(&body).expect("body must be valid JSON");
        assert_eq!(decoded.len(), 2);
    }

    #[tokio::test]
    async fn test_json_render_does_not_clobber_stream_body() {
        use futures_util::stream;

        use crate::http::body::ResBody;

        // Streaming bodies cannot be replaced silently; `Json` must leave them
        // untouched (mirroring `write_body`'s error behavior).
        let mut res = Response::new();
        res.stream(stream::iter(vec![Ok::<_, crate::BoxedError>("chunk")]));
        Json(serde_json::json!({"x": 1})).render(&mut res);
        assert!(matches!(res.body, ResBody::Stream(_)));
    }
}
