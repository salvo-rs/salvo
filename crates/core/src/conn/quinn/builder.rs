//! HTTP/3 support.
use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::future::{Future, pending};
use std::io::{Error as IoError, Result as IoResult};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::task::Poll;

use bytes::Bytes;
use futures_util::future::poll_fn;
use futures_util::stream::{FuturesUnordered, StreamExt};
use salvo_http3::ConnectionState;
use salvo_http3::ext::Protocol;
use salvo_http3::server::RequestStream;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::conn::ctrl::ConnState;
use crate::http::Method;
use crate::http::body::{H3ReqBody, ReqBody, ResBody};
use crate::proto::WebTransportSession;

fn take_unique_arc_extension<T>(
    extensions: &mut http::Extensions,
    name: &'static str,
) -> IoResult<Option<T>>
where
    T: Send + Sync + 'static,
{
    match extensions.remove::<Arc<T>>() {
        Some(value) => Arc::into_inner(value)
            .map(Some)
            .ok_or_else(|| IoError::other(format!("{name} is still shared"))),
        None => Ok(None),
    }
}

/// Builder used to serve HTTP/3 connections.
pub struct Builder {
    inner: salvo_http3::server::Builder,
    pub(crate) auto_alt_svc_header: bool,
}

impl Debug for Builder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder").finish()
    }
}
impl Deref for Builder {
    type Target = salvo_http3::server::Builder;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl DerefMut for Builder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
impl Builder {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        let mut builder = salvo_http3::server::builder();
        builder
            .enable_webtransport(true)
            .enable_extended_connect(true)
            .enable_datagram(true)
            .max_webtransport_sessions(1)
            // h3 0.0.8 can leave aioquic/curl clients waiting for stream end
            // when a GREASE frame is sent just before finishing the response.
            .send_grease(false);
        Self {
            inner: builder,
            auto_alt_svc_header: true,
        }
    }
}

impl Builder {
    /// Configure whether to automatically include the `Alt-Svc` header in HTTP responses.
    ///
    /// If set to `true`, an `Alt-Svc` header will be included in the response.
    /// Note that if an `Alt-Svc` header is already explicitly set in the handlers,
    /// the handler's header will overwrite this automated one.
    ///
    /// The automatically generated header follows this format:
    /// ```text
    /// h3=":{port}"; ma=2592000,h3-29=":{port}"; ma=2592000
    /// ```
    ///
    /// By default, this is set to `true`.
    pub fn auto_alt_svc_header(&mut self, enabled: bool) -> &mut Self {
        self.auto_alt_svc_header = enabled;
        self
    }

    /// Serve an HTTP/3 connection.
    pub async fn serve_connection(
        &self,
        conn: crate::conn::quinn::QuinnConnection,
        hyper_handler: crate::service::HyperHandler,
        graceful_stop_token: Option<CancellationToken>,
    ) -> IoResult<()> {
        let conn_ctrl = hyper_handler.conn_ctrl.clone();
        let crate::conn::quinn::QuinnConnection {
            inner,
            raw: raw_conn,
            handshake,
        } = conn;
        // A connection without a completed handshake has no requests to drain. Close it
        // promptly on graceful stop instead of keeping the server alive until it times out.
        let handshake_completed = tokio::select! {
            completed = handshake => completed,
            _ = async {
                if let Some(token) = &graceful_stop_token {
                    token.cancelled().await;
                } else {
                    pending::<()>().await;
                }
            } => {
                raw_conn.close(0u32.into(), b"server shutting down");
                return Ok(());
            }
        };
        if !handshake_completed {
            raw_conn.close(0u32.into(), b"handshake timed out");
            return Ok(());
        }
        if let Some(tls_info) = crate::conn::TlsInfo::from_quinn(&raw_conn) {
            conn_ctrl.set_tls_info(tls_info);
        }
        let mut conn = self
            .inner
            .build::<salvo_http3::quinn::Connection, bytes::Bytes>(inner)
            .await
            .map_err(|e| IoError::other(format!("invalid connection: {e}")))?;

        // Own every request task for the connection's entire lifetime. Dropping
        // this future (including a server force-stop) or leaving the accept loop
        // aborts pending handlers instead of detaching them from the connection.
        let mut requests = JoinSet::new();
        // Resolving a request waits for its HEADERS frame. Keep those futures
        // owned by the connection. HTTP-only peers resolve them independently;
        // WebTransport peers retain sequential acceptance so early WT streams
        // remain available when their CONNECT handler takes over the connection.
        let mut resolving = FuturesUnordered::new();
        let mut shutting_down = false;
        loop {
            let waiting_headers = !resolving.is_empty();
            let resolved = tokio::select! {
                accepted = poll_fn(|cx| {
                    if waiting_headers {
                        match conn.settings() {
                            Cow::Borrowed(settings) if settings.enable_webtransport() => {
                                return Poll::Pending;
                            }
                            Cow::Owned(_) => {
                                // SETTINGS may arrive after a request on another stream.
                                // Drive their arrival without accepting a second stream,
                                // while the first request can still resolve immediately.
                                return conn.inner.poll_control(cx).map(|result| match result {
                                    Ok(_) => None,
                                    Err(error) => Some(Err(error)),
                                });
                            }
                            Cow::Borrowed(_) => {}
                        }
                    }
                    let accepted = conn.accept();
                    tokio::pin!(accepted);
                    accepted.poll(cx).map(Some)
                }) => {
                    let Some(accepted) = accepted else {
                        continue;
                    };
                    match accepted {
                        Ok(Some(resolver)) => resolving.push(resolver.resolve_request()),
                        Ok(None) => break,
                        Err(error) => {
                            if !error.is_h3_no_error() {
                                tracing::error!("Connection errored with {}", error);
                            }
                            break;
                        }
                    }
                    None
                }
                resolved = resolving.next(), if !resolving.is_empty() => resolved,
                completed = requests.join_next(), if !requests.is_empty() => {
                    if let Some(Err(error)) = completed {
                        tracing::error!(?error, "HTTP/3 request task failed");
                    }
                    None
                }
                state = async {
                    if shutting_down {
                        conn_ctrl.aborted().await
                    } else {
                        conn_ctrl.notified().await
                    }
                } => {
                    match state {
                        ConnState::Abort => {
                            raw_conn.close(0u32.into(), b"aborted by handler");
                            return Ok(());
                        }
                        ConnState::GracefulShutdown => {
                            // Stay abortable while the GOAWAY is sent: a handler may escalate
                            // graceful shutdown to an abort, which must still close promptly.
                            tokio::select! {
                                result = conn.shutdown(0) => {
                                    result.map_err(|e| IoError::other(format!("failed to shutdown HTTP/3 connection: {e}")))?;
                                    shutting_down = true;
                                }
                                _ = conn_ctrl.aborted() => {
                                    raw_conn.close(0u32.into(), b"aborted by handler");
                                    return Ok(());
                                }
                            }
                        }
                        ConnState::Running => {}
                    }
                    None
                }
                _ = async {
                    if let Some(token) = &graceful_stop_token {
                        token.cancelled().await;
                    } else {
                        pending::<()>().await;
                    }
                }, if !shutting_down => {
                    // As in the handler-initiated branch, a handler abort during the GOAWAY
                    // must still tear the connection down immediately.
                    tokio::select! {
                        result = conn.shutdown(0) => {
                            result.map_err(|e| IoError::other(format!("failed to shutdown HTTP/3 connection: {e}")))?;
                            shutting_down = true;
                        }
                        _ = conn_ctrl.aborted() => {
                            raw_conn.close(0u32.into(), b"aborted by handler");
                            return Ok(());
                        }
                    }
                    None
                }
            };
            let Some(resolved) = resolved else {
                continue;
            };
            match resolved {
                Ok((request, stream)) => {
                    let hyper_handler = hyper_handler.clone();
                    tracing::debug!("new request: {:#?}", request);
                    match request.method() {
                        &Method::CONNECT
                            if request.extensions().get::<Protocol>()
                                == Some(&Protocol::WEB_TRANSPORT) =>
                        {
                            let processed = tokio::select! {
                                processed = process_web_transport(
                                    conn,
                                    request,
                                    stream,
                                    hyper_handler,
                                    raw_conn.clone(),
                                ) => processed?,
                                _ = conn_ctrl.aborted() => {
                                    raw_conn.close(0u32.into(), b"aborted by handler");
                                    return Ok(());
                                }
                            };
                            if let Some(c) = processed {
                                conn = c;
                            } else {
                                return Ok(());
                            }
                        }
                        _ => {
                            let request_conn_ctrl = hyper_handler.conn_ctrl.clone();
                            requests.spawn(async move {
                                tokio::select! {
                                    result = process_request(request, stream, hyper_handler) => {
                                        if let Err(error) = result {
                                            tracing::error!(?error, "process request failed");
                                        }
                                    }
                                    _ = request_conn_ctrl.aborted() => {
                                        // The connection loop closes QUIC. End
                                        // the request without retaining the
                                        // deliberately pending service future.
                                    }
                                }
                            });
                        }
                    }
                }
                Err(error) => {
                    tracing::error!("error resolving request: {error:?}");
                }
            }
        }
        Ok(())
    }
}

async fn process_web_transport(
    conn: salvo_http3::server::Connection<salvo_http3::quinn::Connection, Bytes>,
    request: hyper::Request<()>,
    stream: RequestStream<salvo_http3::quinn::BidiStream<Bytes>, Bytes>,
    hyper_handler: crate::service::HyperHandler,
    raw_conn: crate::proto::quinn::Connection,
) -> IoResult<Option<salvo_http3::server::Connection<salvo_http3::quinn::Connection, Bytes>>> {
    let (parts, _body) = request.into_parts();
    let mut request = hyper::Request::from_parts(parts, ReqBody::None);
    request.extensions_mut().insert(Arc::new(Mutex::new(conn)));
    request.extensions_mut().insert(Arc::new(stream));
    request.extensions_mut().insert(raw_conn);

    let mut response = hyper::service::Service::call(&hyper_handler, request)
        .await
        .map_err(|e| IoError::other(format!("failed to call hyper service : {e}")))?;

    if let Some(session) = take_unique_arc_extension::<
        WebTransportSession<salvo_http3::quinn::Connection, Bytes>,
    >(response.extensions_mut(), "WebTransport session")?
    {
        // `WebTransportSession::accept` already sent the successful CONNECT response. Restore the
        // connection without passing its stream through the normal response writer, which would
        // send a second response on the accepted CONNECT stream.
        let (server_conn, _connect_stream) = session.split();
        let conn = server_conn
            .into_inner()
            .map_err(|e| IoError::other(format!("failed to get conn : {e}")))?;
        return Ok(Some(conn));
    }

    let conn = take_unique_arc_extension::<
        Mutex<salvo_http3::server::Connection<salvo_http3::quinn::Connection, Bytes>>,
    >(response.extensions_mut(), "HTTP/3 connection")?
    .map(|c| {
        c.into_inner()
            .map_err(|e| IoError::other(format!("failed to get conn : {e}")))
    })
    .transpose()?;
    let stream = take_unique_arc_extension::<
        salvo_http3::server::RequestStream<salvo_http3::quinn::BidiStream<Bytes>, Bytes>,
    >(response.extensions_mut(), "WebTransport request stream")?;

    let Some(conn) = conn else {
        return Ok(None);
    };
    let Some(mut stream) = stream else {
        return Ok(Some(conn));
    };

    if let Err(error) = send_response(&mut stream, response).await {
        // A rejected CONNECT response still owns the connection. A failure on
        // its stream must not discard it or interrupt sibling requests.
        tracing::error!(?error, "unable to send WebTransport response");
    }

    Ok(Some(conn))
}

#[allow(clippy::future_not_send)]
async fn process_request<S>(
    request: hyper::Request<()>,
    stream: RequestStream<S, Bytes>,
    hyper_handler: crate::service::HyperHandler,
) -> IoResult<()>
where
    S: salvo_http3::quic::BidiStream<Bytes> + Send + Unpin + 'static,
    <S as salvo_http3::quic::BidiStream<Bytes>>::SendStream: Send,
    <S as salvo_http3::quic::BidiStream<Bytes>>::RecvStream: Send + Sync + Unpin,
{
    let (mut tx, rx) = stream.split();
    let (parts, _body) = request.into_parts();
    let request = hyper::Request::from_parts(parts, ReqBody::from(H3ReqBody::new(rx)));

    let response = hyper::service::Service::call(&hyper_handler, request)
        .await
        .map_err(|e| IoError::other(format!("failed to call hyper service : {e}")))?;

    send_response(&mut tx, response).await
}

// Keep the response writer shared by ordinary requests and rejected
// WebTransport CONNECTs. This narrow interface also permits bounded I/O tests.
trait ResponseStream {
    type Error: std::error::Error + Send + Sync + 'static;

    fn send_response(
        &mut self,
        response: http::Response<()>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn send_data(&mut self, data: Bytes) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn send_trailers(
        &mut self,
        trailers: http::HeaderMap,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn finish(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stop_stream(&mut self);
}

impl<S> ResponseStream for RequestStream<S, Bytes>
where
    S: salvo_http3::quic::SendStream<Bytes> + Send,
{
    type Error = salvo_http3::error::StreamError;

    async fn send_response(&mut self, response: http::Response<()>) -> Result<(), Self::Error> {
        Self::send_response(self, response).await
    }

    async fn send_data(&mut self, data: Bytes) -> Result<(), Self::Error> {
        Self::send_data(self, data).await
    }

    async fn send_trailers(&mut self, trailers: http::HeaderMap) -> Result<(), Self::Error> {
        Self::send_trailers(self, trailers).await
    }

    async fn finish(&mut self) -> Result<(), Self::Error> {
        Self::finish(self).await
    }

    fn stop_stream(&mut self) {
        Self::stop_stream(self, salvo_http3::error::Code::H3_INTERNAL_ERROR);
    }
}

async fn send_response<S: ResponseStream + Send>(
    stream: &mut S,
    response: http::Response<ResBody>,
) -> IoResult<()> {
    let (parts, mut body) = response.into_parts();
    let result = async {
        stream
            .send_response(http::Response::from_parts(parts, ()))
            .await
            .map_err(|e| IoError::other(format!("failed to send response: {e}")))?;
        tracing::debug!("response to connection successful");

        while let Some(frame) = body.next().await {
            let frame = frame?;
            if frame.is_data() {
                stream
                    .send_data(frame.into_data().unwrap_or_default())
                    .await
                    .map_err(|e| IoError::other(format!("failed to send data: {e}")))?;
            } else {
                stream
                    .send_trailers(frame.into_trailers().unwrap_or_default())
                    .await
                    .map_err(|e| IoError::other(format!("failed to send trailers: {e}")))?;
            }
        }
        stream
            .finish()
            .await
            .map_err(|e| IoError::other(format!("failed to finish stream: {e}")))
    }
    .await;
    if result.is_err() {
        // Do not signal a successful end for a truncated response. Reset only
        // this response stream, and release the body without polling it again.
        stream.stop_stream();
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::Context;

    use futures_util::Stream;

    use super::*;
    use crate::http::body::{BytesFrame, Frame};

    #[derive(Default)]
    struct MockResponseStream {
        fail_at: Option<&'static str>,
        events: Vec<&'static str>,
        data: Vec<Bytes>,
        trailers: Option<http::HeaderMap>,
    }

    impl MockResponseStream {
        fn record(&mut self, operation: &'static str) -> IoResult<()> {
            self.events.push(operation);
            if self.fail_at == Some(operation) {
                Err(IoError::other(format!("{operation} I/O error")))
            } else {
                Ok(())
            }
        }
    }

    impl ResponseStream for MockResponseStream {
        type Error = IoError;

        async fn send_response(&mut self, _: http::Response<()>) -> IoResult<()> {
            self.record("response")
        }

        async fn send_data(&mut self, data: Bytes) -> IoResult<()> {
            self.data.push(data);
            self.record("data")
        }

        async fn send_trailers(&mut self, trailers: http::HeaderMap) -> IoResult<()> {
            self.trailers = Some(trailers);
            self.record("trailers")
        }

        async fn finish(&mut self) -> IoResult<()> {
            self.record("finish")
        }

        fn stop_stream(&mut self) {
            self.events.push("stop");
        }
    }

    struct BoundedBody {
        frames: VecDeque<IoResult<BytesFrame>>,
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    impl Stream for BoundedBody {
        type Item = IoResult<BytesFrame>;

        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(self.frames.pop_front())
        }
    }

    impl Drop for BoundedBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    fn tracked_response(
        frames: impl IntoIterator<Item = IoResult<BytesFrame>>,
    ) -> (http::Response<ResBody>, Arc<AtomicUsize>, Arc<AtomicBool>) {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let body = BoundedBody {
            frames: frames.into_iter().collect(),
            polls: polls.clone(),
            dropped: dropped.clone(),
        };
        (http::Response::new(ResBody::stream(body)), polls, dropped)
    }

    fn trailers() -> http::HeaderMap {
        let mut trailers = http::HeaderMap::new();
        trailers.insert("x-checksum", "complete".parse().unwrap());
        trailers
    }

    fn complete_body() -> [IoResult<BytesFrame>; 3] {
        [
            Ok(BytesFrame::data("first")),
            Ok(BytesFrame::data("second")),
            Ok(BytesFrame(Frame::trailers(trailers()))),
        ]
    }

    #[tokio::test]
    async fn response_writer_stops_at_first_io_error_and_drops_body() {
        for (operation, expected_polls, expected_events) in [
            ("response", 0, vec!["response", "stop"]),
            ("data", 1, vec!["response", "data", "stop"]),
            (
                "trailers",
                3,
                vec!["response", "data", "data", "trailers", "stop"],
            ),
            (
                "finish",
                4,
                vec!["response", "data", "data", "trailers", "finish", "stop"],
            ),
        ] {
            let mut stream = MockResponseStream {
                fail_at: Some(operation),
                ..Default::default()
            };
            let (response, polls, dropped) = tracked_response(complete_body());

            let error = send_response(&mut stream, response).await.unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains(&format!("{operation} I/O error"))
            );
            assert_eq!(stream.events, expected_events, "{operation}");
            assert_eq!(polls.load(Ordering::SeqCst), expected_polls, "{operation}");
            assert!(dropped.load(Ordering::SeqCst), "{operation}");
        }
    }

    #[tokio::test]
    async fn response_writer_resets_on_body_error_without_reading_later_frames() {
        let mut stream = MockResponseStream::default();
        let (response, polls, dropped) = tracked_response([
            Ok(BytesFrame::data("first")),
            Err(IoError::other("body error")),
            Ok(BytesFrame::data("unread")),
        ]);

        let error = send_response(&mut stream, response).await.unwrap_err();

        assert!(error.to_string().contains("body error"));
        assert_eq!(stream.events, ["response", "data", "stop"]);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn response_writer_finishes_complete_body_and_trailers() {
        let mut stream = MockResponseStream::default();
        let (response, polls, dropped) = tracked_response(complete_body());

        send_response(&mut stream, response).await.unwrap();

        assert_eq!(
            stream.events,
            ["response", "data", "data", "trailers", "finish"]
        );
        assert_eq!(stream.data, [Bytes::from("first"), Bytes::from("second")]);
        assert_eq!(stream.trailers, Some(trailers()));
        assert_eq!(polls.load(Ordering::SeqCst), 4);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn response_writer_finishes_empty_response() {
        let mut stream = MockResponseStream::default();

        send_response(&mut stream, http::Response::new(ResBody::None))
            .await
            .unwrap();

        assert_eq!(stream.events, ["response", "finish"]);
    }

    #[derive(Debug, Eq, PartialEq)]
    struct NonClone(&'static str);

    #[test]
    fn take_unique_arc_extension_returns_owned_value() {
        let mut extensions = http::Extensions::new();
        extensions.insert(Arc::new(NonClone("session")));

        let value = take_unique_arc_extension::<NonClone>(&mut extensions, "test value")
            .expect("unique Arc should be unwrapped");

        assert_eq!(value, Some(NonClone("session")));
        assert!(extensions.get::<Arc<NonClone>>().is_none());
    }

    #[test]
    fn take_unique_arc_extension_rejects_shared_value() {
        let mut extensions = http::Extensions::new();
        let value = Arc::new(NonClone("session"));
        let _shared = value.clone();
        extensions.insert(value);

        let error = take_unique_arc_extension::<NonClone>(&mut extensions, "test value")
            .expect_err("shared Arc should not be silently discarded");

        assert_eq!(error.to_string(), "test value is still shared");
    }
}
