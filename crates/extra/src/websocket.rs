// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
// port from https://github.com/seanmonstar/warp/blob/master/src/filters/ws.rs

//! WebSocket implementation.
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::websocket::WebSocketUpgrade;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Debug, Deserialize, Serialize)]
//! struct User {
//!     id: usize,
//!     name: String,
//! }
//! #[handler]
//! async fn connect(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
//!     let user = req.parse_queries::<User>();
//!     WebSocketUpgrade::new()
//!         .upgrade(req, res, |mut ws| async move {
//!             println!("{user:#?} ");
//!             while let Some(msg) = ws.recv().await {
//!                 let msg = if let Ok(msg) = msg {
//!                     msg
//!                 } else {
//!                     // client disconnected
//!                     return;
//!                 };
//!
//!                 if ws.send(msg).await.is_err() {
//!                     // client disconnected
//!                     return;
//!                 }
//!             }
//!         })
//!         .await
//! }
//!
//! #[handler]
//! async fn index(res: &mut Response) {
//!     res.render(Text::Html(INDEX_HTML));
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(index).push(Router::with_path("ws").goal(connect));
//!
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//!
//! static INDEX_HTML: &str = r#"<!DOCTYPE html>
//! <html>
//!     <head>
//!         <title>WS</title>
//!     </head>
//!     <body>
//!         <h1>WS</h1>
//!         <div id="status">
//!             <p><em>Connecting...</em></p>
//!         </div>
//!         <script>
//!             const status = document.getElementById('status');
//!             const msg = document.getElementById('msg');
//!             const submit = document.getElementById('submit');
//!             const ws = new WebSocket(`ws://${location.host}/ws?id=123&name=chris`);
//!
//!             ws.onopen = function() {
//!                 status.innerHTML = '<p><em>Connected!</em></p>';
//!             };
//!         </script>
//!     </body>
//! </html>
//! "#;
//!```

use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use futures_util::sink::{Sink, SinkExt};
use futures_util::stream::{Stream, StreamExt};
use futures_util::{future, FutureExt, TryFutureExt};
use hyper::upgrade::OnUpgrade;
use salvo_core::http::header::{SEC_WEBSOCKET_VERSION, UPGRADE};
use salvo_core::http::headers::{
    Connection, HeaderMapExt, SecWebsocketAccept, SecWebsocketKey, Upgrade,
};
use salvo_core::http::{StatusCode, StatusError};
use salvo_core::rt::tokio::TokioIo;
use salvo_core::{Error, Request, Response};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::frame::{CloseFrame, Utf8Bytes};
use tokio_tungstenite::tungstenite::protocol::{self, WebSocketConfig};
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::WebSocketStream;

/// Creates a WebSocket Handler.
/// Request:
/// - Method must be `GET`
/// - Header `connection` must be `upgrade`
/// - Header `upgrade` must be `websocket`
/// - Header `sec-websocket-version` must be `13`
/// - Header `sec-websocket-key` must be set.
///
/// Response:
/// - Status of `101 Switching Protocols`
/// - Header `connection: upgrade`
/// - Header `upgrade: websocket`
/// - Header `sec-websocket-accept` with the hash value of the received key.
#[allow(missing_debug_implementations)]
pub struct WebSocketUpgrade {
    config: Option<WebSocketConfig>,
}

impl Default for WebSocketUpgrade {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketUpgrade {
    /// Create new `WebSocketUpgrade`.
    #[inline]
    pub fn new() -> Self {
        WebSocketUpgrade { config: None }
    }

    /// Create new `WebSocketUpgrade` with config.
    #[inline]
    pub fn with_config(config: WebSocketConfig) -> Self {
        WebSocketUpgrade {
            config: Some(config),
        }
    }

    /// The target minimum size of the write buffer to reach before writing the data
    /// to the underlying stream.
    /// The default value is 128 KiB.
    ///
    /// If set to `0` each message will be eagerly written to the underlying stream.
    /// It is often more optimal to allow them to buffer a little, hence the default value.
    #[inline]
    pub fn write_buffer_size(mut self, max: usize) -> Self {
        self.config
            .get_or_insert_with(WebSocketConfig::default)
            .write_buffer_size = max;
        self
    }

    /// The max size of the write buffer in bytes. Setting this can provide backpressure
    /// in the case the write buffer is filling up due to write errors.
    /// The default value is unlimited.
    ///
    /// **Note**: The write buffer only builds up past [`write_buffer_size`](Self::write_buffer_size)
    /// when writes to the underlying stream are failing. So the **write buffer can not
    /// fill up if you are not observing write errors even if not flushing**.
    ///
    /// Should always be at least [`write_buffer_size + 1 message`](Self::write_buffer_size)
    /// and probably a little more depending on error handling strategy.
    #[inline]
    pub fn max_write_buffer_size(mut self, max: usize) -> Self {
        self.config
            .get_or_insert_with(WebSocketConfig::default)
            .max_write_buffer_size = max;
        self
    }

    /// The maximum size of a message. `None` means no size limit. The default value is 64 MiB
    /// which should be reasonably big for all normal use-cases but small enough to prevent
    /// memory eating by a malicious user.
    #[inline]
    pub fn max_message_size(mut self, max: usize) -> Self {
        self.config
            .get_or_insert_with(WebSocketConfig::default)
            .max_message_size = Some(max);
        self
    }

    /// The maximum size of a single message frame. `None` means no size limit. The limit is for
    /// frame payload NOT including the frame header. The default value is 16 MiB which should
    /// be reasonably big for all normal use-cases but small enough to prevent memory eating
    /// by a malicious user.
    #[inline]
    pub fn max_frame_size(mut self, max: usize) -> Self {
        self.config
            .get_or_insert_with(WebSocketConfig::default)
            .max_frame_size = Some(max);
        self
    }

    /// When set to `true`, the server will accept and handle unmasked frames
    /// from the client. According to the RFC 6455, the server must close the
    /// connection to the client in such cases, however it seems like there are
    /// some popular libraries that are sending unmasked frames, ignoring the RFC.
    /// By default this option is set to `false`, i.e. according to RFC 6455.
    #[inline]
    pub fn accept_unmasked_frames(mut self, accept: bool) -> Self {
        self.config
            .get_or_insert_with(WebSocketConfig::default)
            .accept_unmasked_frames = accept;
        self
    }

    /// Upgrade websocket request.
    pub async fn upgrade<F, Fut>(
        &self,
        req: &mut Request,
        res: &mut Response,
        callback: F,
    ) -> Result<(), StatusError>
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let req_headers = req.headers();
        let matched = req_headers
            .typed_get::<Connection>()
            .map(|conn| conn.contains(UPGRADE))
            .unwrap_or(false);
        if !matched {
            tracing::debug!("missing connection upgrade");
            return Err(StatusError::bad_request().brief("Missing connection upgrade."));
        }
        let matched = req_headers
            .get(UPGRADE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_lowercase() == "websocket")
            .unwrap_or(false);
        if !matched {
            tracing::debug!("missing upgrade header or it is not equal websocket");
            return Err(StatusError::bad_request()
                .brief("Missing upgrade header or it is not equal websocket."));
        }
        let matched = !req_headers
            .get(SEC_WEBSOCKET_VERSION)
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "13")
            .unwrap_or(false);
        if matched {
            tracing::debug!("websocket version is not equal 13");
            return Err(StatusError::bad_request().brief("Websocket version is not equal 13."));
        }
        let sec_ws_key = if let Some(key) = req_headers.typed_get::<SecWebsocketKey>() {
            key
        } else {
            tracing::debug!("sec_websocket_key is not exist in request headers");
            return Err(StatusError::bad_request()
                .brief("sec_websocket_key is not exist in request headers."));
        };

        res.status_code(StatusCode::SWITCHING_PROTOCOLS);

        res.headers_mut().typed_insert(Connection::upgrade());
        res.headers_mut().typed_insert(Upgrade::websocket());
        res.headers_mut()
            .typed_insert(SecWebsocketAccept::from(sec_ws_key));

        if let Some(on_upgrade) = req.extensions_mut().remove::<OnUpgrade>() {
            let config = self.config;
            tokio::spawn(async move {
                let socket = on_upgrade
                    .and_then(move |upgraded| {
                        tracing::debug!("websocket upgrade complete");
                        WebSocket::from_raw_socket(upgraded, protocol::Role::Server, config).map(Ok)
                    })
                    .await
                    .expect("connection upgrade failed");
                callback(socket).await;
            });
            Ok(())
        } else {
            tracing::debug!("websocket couldn't be upgraded since no upgrade state was present");
            Err(StatusError::bad_request()
                .brief("Websocket couldn't be upgraded since no upgrade state was present."))
        }
    }
}

/// A websocket `Stream` and `Sink`, provided to `ws` filters.
///
/// Ping messages sent from the client will be handled internally by replying with a Pong message.
/// Close messages need to be handled explicitly: usually by closing the `Sink` end of the
/// `WebSocket`.
pub struct WebSocket {
    inner: WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
}

impl WebSocket {
    #[inline]
    pub(crate) async fn from_raw_socket(
        upgraded: hyper::upgrade::Upgraded,
        role: protocol::Role,
        config: Option<protocol::WebSocketConfig>,
    ) -> Self {
        WebSocketStream::from_raw_socket(TokioIo::new(upgraded), role, config)
            .map(|inner| WebSocket { inner })
            .await
    }

    /// Receive another message.
    ///
    /// Returns `None` if the stream has closed.
    pub async fn recv(&mut self) -> Option<Result<Message, Error>> {
        self.next().await
    }

    /// Send a message.
    pub async fn send(&mut self, msg: Message) -> Result<(), Error> {
        self.inner.send(msg.inner).await.map_err(Error::other)
    }

    /// Gracefully close this websocket.
    #[inline]
    pub async fn close(mut self) -> Result<(), Error> {
        future::poll_fn(|cx| Pin::new(&mut self).poll_close(cx)).await
    }
}

impl Stream for WebSocket {
    type Item = Result<Message, Error>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match ready!(Pin::new(&mut self.inner).poll_next(cx)) {
            Some(Ok(item)) => Poll::Ready(Some(Ok(Message { inner: item }))),
            Some(Err(e)) => {
                tracing::debug!("websocket poll error: {}", e);
                Poll::Ready(Some(Err(Error::other(e))))
            }
            None => {
                tracing::debug!("websocket closed");
                Poll::Ready(None)
            }
        }
    }
}

impl Sink<Message> for WebSocket {
    type Error = Error;

    #[inline]
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner)
            .poll_ready(cx)
            .map_err(Error::other)
    }

    #[inline]
    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        Pin::new(&mut self.inner)
            .start_send(item.inner)
            .map_err(Error::other)
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner)
            .poll_flush(cx)
            .map_err(Error::other)
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner)
            .poll_close(cx)
            .map_err(Error::other)
    }
}

impl Debug for WebSocket {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("WebSocket").finish()
    }
}

/// A WebSocket message.
///
/// This will likely become a `non-exhaustive` enum in the future, once that
/// language feature has stabilized.
#[derive(Eq, PartialEq, Clone)]
pub struct Message {
    inner: protocol::Message,
}

impl Message {
    /// Construct a new Text `Message`.
    #[inline]
    pub fn text<S: Into<Utf8Bytes>>(s: S) -> Message {
        Message {
            inner: protocol::Message::text(s),
        }
    }

    /// Construct a new Binary `Message`.
    #[inline]
    pub fn binary<V: Into<Bytes>>(v: V) -> Message {
        Message {
            inner: protocol::Message::binary(v),
        }
    }

    /// Construct a new Ping `Message`.
    #[inline]
    pub fn ping<V: Into<Bytes>>(v: V) -> Message {
        Message {
            inner: protocol::Message::Ping(v.into()),
        }
    }

    /// Construct a new Pong `Message`.
    #[inline]
    pub fn pong<V: Into<Bytes>>(v: V) -> Message {
        Message {
            inner: protocol::Message::Pong(v.into()),
        }
    }

    /// Construct the default Close `Message`.
    #[inline]
    pub fn close() -> Message {
        Message {
            inner: protocol::Message::Close(None),
        }
    }

    /// Construct a Close `Message` with a code and reason.
    #[inline]
    pub fn close_with(code: impl Into<u16>, reason: impl Into<Utf8Bytes>) -> Message {
        Message {
            inner: protocol::Message::Close(Some(CloseFrame {
                code: CloseCode::from(code.into()),
                reason: reason.into(),
            })),
        }
    }

    /// Returns true if this message is a Text message.
    #[inline]
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    /// Returns true if this message is a Binary message.
    #[inline]
    pub fn is_binary(&self) -> bool {
        self.inner.is_binary()
    }

    /// Returns true if this message a is a Close message.
    #[inline]
    pub fn is_close(&self) -> bool {
        self.inner.is_close()
    }

    /// Returns true if this message is a Ping message.
    #[inline]
    pub fn is_ping(&self) -> bool {
        self.inner.is_ping()
    }

    /// Returns true if this message is a Pong message.
    #[inline]
    pub fn is_pong(&self) -> bool {
        self.inner.is_pong()
    }

    /// Try to get the close frame (close code and reason).
    #[inline]
    pub fn close_frame(&self) -> Option<(u16, &str)> {
        if let protocol::Message::Close(Some(ref close_frame)) = self.inner {
            Some((close_frame.code.into(), close_frame.reason.as_ref()))
        } else {
            None
        }
    }

    /// Try to get a reference to the string text, if this is a Text message.
    #[inline]
    pub fn as_str(&self) -> Result<&str, Error> {
        match &self.inner {
            protocol::Message::Text(s) => Ok(s.as_str()),
            _ => Err(Error::Other("not a text message".into())),
        }
    }

    /// Returns the bytes of this message, if the message can contain data.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.inner {
            protocol::Message::Text(s) => s.as_bytes(),
            protocol::Message::Binary(v) => v.as_ref(),
            protocol::Message::Ping(v) => v.as_ref(),
            protocol::Message::Pong(v) => v.as_ref(),
            protocol::Message::Close(_) => &[],
            protocol::Message::Frame(v) => v.payload(),
        }
    }
}

impl Debug for Message {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

#[allow(clippy::from_over_into)]
impl Into<Vec<u8>> for Message {
    #[inline]
    fn into(self) -> Vec<u8> {
        self.as_bytes().into()
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::conn::{Acceptor, Listener};
    use salvo_core::http::header::*;
    use salvo_core::prelude::*;
    use salvo_core::rt::tokio::TokioIo;

    use super::*;

    #[handler]
    async fn connect(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
        WebSocketUpgrade::new()
            .upgrade(req, res, |mut ws| async move {
                while let Some(msg) = ws.recv().await {
                    let msg = if let Ok(msg) = msg {
                        msg
                    } else {
                        return;
                    };

                    if ws.send(msg).await.is_err() {
                        return;
                    }
                }
            })
            .await
    }

    #[tokio::test]
    async fn test_websocket() {
        let router = Router::new().goal(connect);
        let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();

        tokio::spawn(async move {
            Server::new(acceptor).serve(router).await;
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .unwrap();
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        // Fetch the url...
        let req = hyper::Request::builder()
            .uri(format!("http://{}", addr))
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "Upgrade")
            .header(SEC_WEBSOCKET_KEY, "6D69KGBOr4Re+Nj6zx9aQA==")
            .header(SEC_WEBSOCKET_VERSION, "13")
            .body(http_body_util::Empty::<hyper::body::Bytes>::new())
            .unwrap();

        let res = sender.send_request(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::SWITCHING_PROTOCOLS);
    }
}
