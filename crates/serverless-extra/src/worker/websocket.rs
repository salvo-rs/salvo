//! Cloudflare Workers websocket compatibility layer.
//!
//! The API mirrors `salvo_extra::websocket` so a handler written for a
//! standard Salvo server can be reused in a Workers deployment:
//!
//! ```rust,ignore
//! use salvo_core::prelude::*;
//! use salvo_serverless_extra::worker::websocket::WebSocketUpgrade;
//!
//! #[handler]
//! async fn connect(
//!     req: &mut Request,
//!     _depot: &mut Depot,
//!     res: &mut Response,
//!     _ctrl: &mut FlowCtrl,
//! ) -> Result<(), StatusError> {
//!     WebSocketUpgrade::new()
//!         .upgrade(req, res, |mut ws| async move {
//!             while let Some(msg) = ws.recv().await {
//!                 let msg = if let Ok(msg) = msg { msg } else { return };
//!                 if ws.send(msg).await.is_err() {
//!                     return;
//!                 }
//!             }
//!         })
//!         .await
//! }
//! ```

use core::fmt::{self, Debug, Formatter};
use core::future::Future;

use base64::Engine as _;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::stream::StreamExt;
use http::HeaderValue;
use http::header::{CONNECTION, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};
use salvo_core::http::headers::{Connection, HeaderMapExt};
use salvo_core::http::{Method, StatusCode, StatusError};
use salvo_core::{Error, Request, Response};
use salvo_serverless::worker::WorkerWebSocketHandle;
use worker::wasm_bindgen_futures::spawn_local;
use worker::{WebSocket as WorkerWebSocket, WebSocketPair, WebsocketEvent};

/// Upgrades a Workers request to a websocket connection.
///
/// The handshake checks follow RFC 6455 and match the standard Salvo
/// websocket upgrader: the request must be `GET`, carry a `Connection` token
/// of `upgrade`, an `Upgrade: websocket` header, a single
/// `Sec-WebSocket-Version: 13` header and a single base64-encoded 16-byte
/// `Sec-WebSocket-Key` header.
#[must_use]
pub struct WebSocketUpgrade;

impl Default for WebSocketUpgrade {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketUpgrade {
    /// Creates a new `WebSocketUpgrade`.
    #[inline]
    pub fn new() -> Self {
        Self
    }

    /// Upgrades the request and passes the resulting websocket to `callback`.
    ///
    /// A `WebSocketPair` is created on the Workers side: the client handle is
    /// stored in the response extensions so the response converter can build
    /// a `worker::Response` from it, while the server handle drives the
    /// callback. The event pump and the callback are merged into a single
    /// future and registered through `Context::wait_until` (falling back to
    /// `spawn_local` when no context is attached) so workerd tracks the
    /// websocket as one long-lived task and does not cancel it as a hung
    /// request after the `101 Switching Protocols` response is returned.
    ///
    /// # Errors
    ///
    /// Returns a [`StatusError`] when the handshake is invalid, the
    /// `WebSocketPair` cannot be created, or the server socket cannot accept
    /// the connection.
    // 保留 `async` 以与 `salvo_extra::websocket` 的 API 保持一致；底层 Workers
    // 调用是同步的，因此函数体内没有 `.await`。
    // `async` is kept for API parity with `salvo_extra::websocket`; the
    // underlying Workers calls are synchronous, hence no `.await` in the body.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
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
        validate_handshake(req, res)?;

        let pair = WebSocketPair::new().map_err(internal_error)?;
        let server = pair.server;
        let client = pair.client;

        server.accept().map_err(internal_error)?;

        let (tx, rx): (
            UnboundedSender<Result<Message, Error>>,
            UnboundedReceiver<Result<Message, Error>>,
        ) = unbounded();

        // 事件泵：从 Workers 事件流读取消息并转发到内部 channel。
        // The event pump reads from the Workers event stream and forwards
        // messages to the internal channel.
        let reader = server.clone();
        let pump = async move {
            let mut events = match reader.events() {
                Ok(events) => events,
                Err(err) => {
                    let _ = tx.unbounded_send(Err(Error::other(err)));
                    return;
                }
            };
            while let Some(event) = events.next().await {
                match event {
                    Ok(WebsocketEvent::Message(message)) => {
                        let message = if let Some(text) = message.text() {
                            Message::text(text)
                        } else if let Some(bytes) = message.bytes() {
                            Message::binary(bytes)
                        } else {
                            continue;
                        };
                        if tx.unbounded_send(Ok(message)).is_err() {
                            break;
                        }
                    }
                    Ok(WebsocketEvent::Close(event)) => {
                        // 收到对端关闭帧后回发关闭帧，完成 RFC 6455 关闭握手。
                        // 若服务端 socket 不显式关闭，连接会处于半开状态，触发
                        // workerd 的挂起检测并取消请求。
                        // Reply with a close frame once the peer's close frame
                        // arrives to finish the RFC 6455 close handshake. Leaving
                        // the server socket open leaves the connection half-open
                        // and trips workerd's hang watchdog.
                        let code = event.code();
                        let result = if code == 0 {
                            reader.close(None, None::<String>)
                        } else {
                            reader.close(Some(code), Some(event.reason()))
                        };
                        let _ = result;
                        break;
                    }
                    Err(err) => {
                        let _ = tx.unbounded_send(Err(Error::other(err)));
                        break;
                    }
                }
            }
        };

        let socket = WebSocket { server, rx };
        let callback = callback(socket);

        // 事件泵与用户回调合并为单个 future，避免两个独立任务在 workerd
        // 中因无生命周期关联而被挂起检测误判。
        // Merge the event pump and user callback into a single future so workerd
        // can track them as one long-lived task instead of two detached ones.
        let combined = async move {
            futures_util::future::join(pump, callback).await;
        };

        // 通过 `spawn_local` 调度合并 future。WebSocket 连接本身会让 workerd
        // 保持 isolate 存活，无需 `Context::wait_until`。worker-rs 0.9.0 的
        // `wait_until` 会用 `Instrumented` 包装 future，并在 poll 时调用运行时
        // 的 `withActiveSpan`，而本地 wrangler 4.x 的 workerd 未实现该 API，
        // 会触发 `arg0.withActiveSpan is not a function` panic 并进而被挂起
        // 检测取消请求，因此这里统一改用 `spawn_local` 规避。
        // Schedule the merged future via `spawn_local`. The websocket connection
        // itself keeps the isolate alive, so `Context::wait_until` is unnecessary.
        // worker-rs 0.9.0 wraps `wait_until` futures in `Instrumented`, which calls
        // the runtime's `withActiveSpan` on poll; local wrangler 4.x workerd lacks
        // that API, panicking with `arg0.withActiveSpan is not a function` and then
        // being cancelled by the hung-request watchdog. Use `spawn_local` instead.
        spawn_local(combined);

        res.status_code(StatusCode::SWITCHING_PROTOCOLS);
        res.headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("upgrade"));
        res.headers_mut()
            .insert(UPGRADE, HeaderValue::from_static("websocket"));
        res.extensions.insert(WorkerWebSocketHandle(client));

        Ok(())
    }
}

impl Debug for WebSocketUpgrade {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketUpgrade").finish()
    }
}

fn validate_handshake(req: &Request, res: &mut Response) -> Result<(), StatusError> {
    if req.method() != Method::GET {
        return Err(StatusError::bad_request().brief("websocket handshake requires GET"));
    }

    let headers = req.headers();
    let matched = headers
        .typed_get::<Connection>()
        .is_some_and(|conn| conn.contains(UPGRADE));
    if !matched {
        return Err(StatusError::bad_request().brief("missing connection upgrade"));
    }

    let matched = headers
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if !matched {
        return Err(
            StatusError::bad_request().brief("missing upgrade header or it is not websocket")
        );
    }

    let invalid_version = headers.get_all(SEC_WEBSOCKET_VERSION).iter().count() != 1
        || headers
            .get(SEC_WEBSOCKET_VERSION)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| value != "13");
    if invalid_version {
        res.add_header(SEC_WEBSOCKET_VERSION, "13", true)
            .expect("static version is valid");
        return Err(StatusError::bad_request().brief("websocket version is not 13"));
    }

    let mut keys = headers.get_all(SEC_WEBSOCKET_KEY).iter();
    let Some(first_key) = keys.next() else {
        return Err(StatusError::bad_request().brief("missing sec-websocket-key header"));
    };
    let key = first_key
        .to_str()
        .ok()
        .and_then(|key| {
            base64::engine::general_purpose::STANDARD
                .decode(key.trim_matches([' ', '\t']))
                .ok()
        })
        .and_then(|key| <[u8; 16]>::try_from(key).ok());
    if key.as_ref().is_none_or(|_| keys.next().is_some()) {
        return Err(
            StatusError::bad_request().brief("websocket requires one base64-encoded 16-byte key")
        );
    }

    Ok(())
}

fn internal_error(err: worker::Error) -> StatusError {
    StatusError::internal_server_error()
        .brief(err.to_string())
        .cause(err)
}

/// A websocket connection handed to an upgrade callback.
///
/// Messages are buffered in an unbounded channel fed by the Workers event
/// stream, so [`recv`](Self::recv) yields the same text and binary messages
/// as the standard Salvo websocket.
pub struct WebSocket {
    server: WorkerWebSocket,
    rx: UnboundedReceiver<Result<Message, Error>>,
}

impl WebSocket {
    /// Receives the next message, or `None` when the connection is closed.
    pub async fn recv(&mut self) -> Option<Result<Message, Error>> {
        self.rx.next().await
    }

    /// Sends a message to the peer.
    ///
    /// A close message sends an RFC 6455 close frame. When the close frame
    /// carries a code and reason they are forwarded to the Workers socket via
    /// `close_with_code_and_reason`; otherwise a plain close is sent.
    ///
    /// # Errors
    ///
    /// Returns an error when the Workers socket rejects the send operation.
    // 保留 `async` 以与 `salvo_extra::websocket` 的 API 保持一致；底层 Workers
    // 调用是同步的，因此函数体内没有 `.await`。
    // `async` is kept for API parity with `salvo_extra::websocket`; the
    // underlying Workers calls are synchronous, hence no `.await` in the body.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn send(&mut self, msg: Message) -> Result<(), Error> {
        match msg.inner {
            MessageInner::Text(text) => self.server.send_with_str(&text).map_err(Error::other),
            MessageInner::Binary(bytes) => {
                self.server.send_with_bytes(&bytes).map_err(Error::other)
            }
            MessageInner::Close(frame) => {
                let (code, reason) = match frame {
                    Some((code, reason)) => (Some(code), Some(reason)),
                    None => (None, None),
                };
                self.server
                    .close(code, reason.as_deref())
                    .map_err(Error::other)
            }
        }
    }

    /// Gracefully closes the connection.
    ///
    /// # Errors
    ///
    /// Returns an error when the Workers socket rejects the close frame.
    // 保留 `async` 以与 `salvo_extra::websocket` 的 API 保持一致；底层 Workers
    // 调用是同步的，因此函数体内没有 `.await`。
    // `async` is kept for API parity with `salvo_extra::websocket`; the
    // underlying Workers calls are synchronous, hence no `.await` in the body.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    #[inline]
    pub async fn close(self) -> Result<(), Error> {
        self.server.close(None, None::<&str>).map_err(Error::other)
    }
}

impl Debug for WebSocket {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocket").finish()
    }
}

/// A text, binary or close websocket message.
#[derive(Clone, Eq, PartialEq)]
pub struct Message {
    inner: MessageInner,
}

#[derive(Clone, Eq, PartialEq)]
enum MessageInner {
    Text(String),
    Binary(Vec<u8>),
    Close(Option<(u16, String)>),
}

impl Message {
    /// Creates a text message.
    #[inline]
    #[must_use]
    pub fn text<S: Into<String>>(s: S) -> Self {
        Self {
            inner: MessageInner::Text(s.into()),
        }
    }

    /// Creates a binary message.
    #[inline]
    #[must_use]
    pub fn binary<V: Into<Vec<u8>>>(v: V) -> Self {
        Self {
            inner: MessageInner::Binary(v.into()),
        }
    }

    /// Creates the default close message without a code or reason.
    #[inline]
    #[must_use]
    pub fn close() -> Self {
        Self {
            inner: MessageInner::Close(None),
        }
    }

    /// Creates a close message with a status code and reason.
    #[inline]
    #[must_use]
    pub fn close_with(code: impl Into<u16>, reason: impl Into<String>) -> Self {
        Self {
            inner: MessageInner::Close(Some((code.into(), reason.into()))),
        }
    }

    /// Returns the close code and reason when this is a close frame with both.
    #[inline]
    #[must_use]
    pub fn close_frame(&self) -> Option<(u16, &str)> {
        match &self.inner {
            MessageInner::Close(Some((code, reason))) => Some((*code, reason.as_str())),
            _ => None,
        }
    }

    /// Returns true if this is a text message.
    #[inline]
    #[must_use]
    pub fn is_text(&self) -> bool {
        matches!(self.inner, MessageInner::Text(_))
    }

    /// Returns true if this is a binary message.
    #[inline]
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(self.inner, MessageInner::Binary(_))
    }

    /// Returns true if this is a close message.
    #[inline]
    #[must_use]
    pub fn is_close(&self) -> bool {
        matches!(self.inner, MessageInner::Close(_))
    }

    /// Returns the payload as a string slice when this is a text message.
    ///
    /// # Errors
    ///
    /// Returns an error when the message is not a text message.
    #[inline]
    pub fn as_str(&self) -> Result<&str, Error> {
        match &self.inner {
            MessageInner::Text(s) => Ok(s.as_str()),
            _ => Err(Error::other("not a text message")),
        }
    }

    /// Returns the payload bytes; a close message returns an empty slice.
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.inner {
            MessageInner::Text(s) => s.as_bytes(),
            MessageInner::Binary(b) => b.as_slice(),
            MessageInner::Close(_) => &[],
        }
    }
}

impl Debug for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.inner {
            MessageInner::Text(s) => f.debug_tuple("Text").field(s).finish(),
            MessageInner::Binary(b) => f.debug_tuple("Binary").field(b).finish(),
            MessageInner::Close(frame) => f.debug_tuple("Close").field(frame).finish(),
        }
    }
}

impl From<Message> for Vec<u8> {
    #[inline]
    fn from(msg: Message) -> Self {
        msg.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use http::header::{CONNECTION, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};
    use salvo_core::http::{Method, Response};

    use super::*;

    fn valid_request() -> Request {
        let mut req = Request::new();
        req.headers_mut()
            .insert(CONNECTION, "keep-alive, Upgrade".parse().unwrap());
        req.headers_mut()
            .insert(UPGRADE, "websocket".parse().unwrap());
        req.headers_mut()
            .insert(SEC_WEBSOCKET_VERSION, "13".parse().unwrap());
        req.headers_mut().insert(
            SEC_WEBSOCKET_KEY,
            "dGhlIHNhbXBsZSBub25jZQ==".parse().unwrap(),
        );
        req
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn accepts_valid_handshake() {
        let req = valid_request();
        let mut res = Response::new();
        assert!(validate_handshake(&req, &mut res).is_ok());
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn rejects_non_get_request() {
        let mut req = valid_request();
        *req.method_mut() = Method::POST;
        let mut res = Response::new();
        assert!(validate_handshake(&req, &mut res).is_err());
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn rejects_missing_connection_upgrade() {
        let mut req = valid_request();
        req.headers_mut()
            .insert(CONNECTION, "keep-alive".parse().unwrap());
        let mut res = Response::new();
        assert!(validate_handshake(&req, &mut res).is_err());
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn rejects_wrong_version() {
        let mut req = valid_request();
        req.headers_mut()
            .insert(SEC_WEBSOCKET_VERSION, "12".parse().unwrap());
        let mut res = Response::new();
        assert!(validate_handshake(&req, &mut res).is_err());
        assert_eq!(res.headers().get(SEC_WEBSOCKET_VERSION).unwrap(), "13");
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn rejects_invalid_key() {
        let mut req = valid_request();
        req.headers_mut()
            .insert(SEC_WEBSOCKET_KEY, "not-base64".parse().unwrap());
        let mut res = Response::new();
        assert!(validate_handshake(&req, &mut res).is_err());
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn default_close_message_has_no_frame() {
        let msg = Message::close();
        assert!(msg.is_close());
        assert!(!msg.is_text());
        assert!(!msg.is_binary());
        assert_eq!(msg.close_frame(), None);
        assert!(msg.as_bytes().is_empty());
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn close_with_message_carries_code_and_reason() {
        let msg = Message::close_with(1000u16, "bye");
        assert!(msg.is_close());
        assert_eq!(msg.close_frame(), Some((1000, "bye")));
        assert!(msg.as_bytes().is_empty());
    }

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn text_and_binary_messages_report_kinds() {
        let text = Message::text("hello");
        assert!(text.is_text());
        assert_eq!(text.as_str().unwrap(), "hello");
        assert_eq!(text.as_bytes(), b"hello");

        let binary = Message::binary(vec![1u8, 2, 3]);
        assert!(binary.is_binary());
        assert_eq!(binary.as_bytes(), &[1, 2, 3]);
        assert!(binary.as_str().is_err());
        assert!(binary.close_frame().is_none());
    }
}
