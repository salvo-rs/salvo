//! HTTP/3 support.
use std::fmt::{self, Debug, Formatter};
use std::future::pending;
use std::io::{Error as IoError, Result as IoResult};
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures_util::Stream;
use futures_util::future::poll_fn;
use salvo_http3::ext::Protocol;
use salvo_http3::server::RequestStream;
use tokio_util::sync::CancellationToken;

use crate::conn::ctrl::ConnState;
use crate::http::Method;
use crate::http::body::{H3ReqBody, ReqBody};
use crate::proto::WebTransportSession;

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
        let raw_conn = conn.quinn().clone();
        let mut conn = self
            .inner
            .build::<salvo_http3::quinn::Connection, bytes::Bytes>(conn.into_inner())
            .await
            .map_err(|e| IoError::other(format!("invalid connection: {e}")))?;

        let mut shutting_down = false;
        loop {
            let accepted = tokio::select! {
                accepted = conn.accept() => Some(accepted),
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
            let Some(accepted) = accepted else {
                continue;
            };
            match accepted {
                Ok(Some(resolver)) => {
                    let hyper_handler = hyper_handler.clone();
                    // Keep the connection abortable while the client sends the request head. A
                    // stream that stalls before its headers arrive must not block a handler on
                    // another stream from tearing the QUIC connection down.
                    let resolved = tokio::select! {
                        resolved = resolver.resolve_request() => resolved,
                        _ = conn_ctrl.aborted() => {
                            raw_conn.close(0u32.into(), b"aborted by handler");
                            return Ok(());
                        }
                    };
                    let (request, stream) = match resolved {
                        Ok(request) => request,
                        Err(err) => {
                            tracing::error!("error resolving request: {err:?}");
                            continue;
                        }
                    };
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
                            tokio::spawn(async move {
                                tokio::select! {
                                    result = process_request(request, stream, hyper_handler) => {
                                        if let Err(error) = result {
                                            tracing::error!(?error, "process request failed");
                                        }
                                    }
                                    _ = request_conn_ctrl.aborted() => {
                                        // The connection loop closes QUIC. Ending
                                        // this detached task avoids retaining the
                                        // deliberately pending service future.
                                    }
                                }
                            });
                        }
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    if !e.is_h3_no_error() {
                        tracing::error!("Connection errored with {}", e);
                    }
                    break;
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

    let conn;
    let stream;
    if let Some(session) = response
        .extensions_mut()
        .remove::<WebTransportSession<salvo_http3::quinn::Connection, Bytes>>()
    {
        let (server_conn, connect_stream) = session.split();

        conn = Some(
            server_conn
                .into_inner()
                .map_err(|e| IoError::other(format!("failed to get conn : {e}")))?,
        );
        stream = Some(connect_stream);
    } else {
        conn = response
            .extensions_mut()
            .remove::<Arc<Mutex<salvo_http3::server::Connection<salvo_http3::quinn::Connection, Bytes>>>>()
            .map(|c| {
                Arc::into_inner(c).expect("HTTP/3 connection must exist").into_inner()
                    .map_err(|e| IoError::other( format!("failed to get conn : {e}")))
            })
            .transpose()?;
        stream =
            response
                .extensions_mut()
                .remove::<Arc<
                    salvo_http3::server::RequestStream<
                        salvo_http3::quinn::BidiStream<Bytes>,
                        Bytes,
                    >,
                >>()
                .and_then(Arc::into_inner);
    }

    let Some(conn) = conn else {
        return Ok(None);
    };
    let Some(mut stream) = stream else {
        return Ok(Some(conn));
    };

    let (parts, mut body) = response.into_parts();
    let empty_res = http::Response::from_parts(parts, ());
    match stream.send_response(empty_res).await {
        Ok(_) => {
            tracing::debug!("response to connection successful");
        }
        Err(e) => {
            tracing::error!(error = ?e, "unable to send response to connection peer");
        }
    }

    let mut body = Pin::new(&mut body);
    while let Some(result) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        match result {
            Ok(frame) => {
                if frame.is_data() {
                    if let Err(e) = stream
                        .send_data(frame.into_data().unwrap_or_default())
                        .await
                    {
                        tracing::error!(error = ?e, "unable to send data to connection peer");
                    }
                } else if let Err(e) = stream
                    .send_trailers(frame.into_trailers().unwrap_or_default())
                    .await
                {
                    tracing::error!(error = ?e, "unable to send trailers to connection peer");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "unable to poll data from connection");
            }
        }
    }
    stream
        .finish()
        .await
        .map_err(|e| IoError::other(format!("failed to finish stream : {e}")))?;

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
    <S as salvo_http3::quic::BidiStream<Bytes>>::RecvStream: Send + Sync + Unpin,
{
    let (mut tx, rx) = stream.split();
    let (parts, _body) = request.into_parts();
    let request = hyper::Request::from_parts(parts, ReqBody::from(H3ReqBody::new(rx)));

    let response = hyper::service::Service::call(&hyper_handler, request)
        .await
        .map_err(|e| IoError::other(format!("failed to call hyper service : {e}")))?;

    let (parts, mut body) = response.into_parts();
    let empty_res = http::Response::from_parts(parts, ());
    match tx.send_response(empty_res).await {
        Ok(_) => {
            tracing::debug!("response to connection successful");
        }
        Err(e) => {
            tracing::error!(error = ?e, "unable to send response to connection peer");
        }
    }

    let mut body = Pin::new(&mut body);
    while let Some(result) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        match result {
            Ok(frame) => {
                if frame.is_data() {
                    if let Err(e) = tx.send_data(frame.into_data().unwrap_or_default()).await {
                        tracing::error!(error = ?e, "unable to send data to connection peer");
                    }
                } else if let Err(e) = tx
                    .send_trailers(frame.into_trailers().unwrap_or_default())
                    .await
                {
                    tracing::error!(error = ?e, "unable to send trailers to connection peer");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "unable to poll data from connection");
            }
        }
    }
    tx.finish()
        .await
        .map_err(|e| IoError::other(format!("failed to finish stream : {e}")))
}
