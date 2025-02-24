//! Client implementation of the HTTP/3 protocol

use std::{
    marker::PhantomData,
    sync::{atomic::AtomicUsize, Arc},
    task::{Context, Poll, Waker},
};

use bytes::{Buf, Bytes, BytesMut};
use futures_util::future;
use http::{request, HeaderMap, Response};
use tracing::{info, trace};

use crate::{
    connection::{self, ConnectionInner, ConnectionState, SharedStateRef},
    error::{Code, Error, ErrorLevel},
    frame::FrameStream,
    proto::{frame::Frame, headers::Header, varint::VarInt},
    qpack, quic, stream,
};

/// Start building a new HTTP/3 client
pub fn builder() -> Builder {
    Builder::new()
}

/// Create a new HTTP/3 client with default settings
pub async fn new<C, O>(conn: C) -> Result<(Connection<C, Bytes>, SendRequest<O, Bytes>), Error>
where
    C: quic::Connection<Bytes, OpenStreams = O>,
    O: quic::OpenStreams<Bytes>,
{
    //= https://www.rfc-editor.org/rfc/rfc9114#section-3.3
    //= type=implication
    //# Clients SHOULD NOT open more than one HTTP/3 connection to a given IP
    //# address and UDP port, where the IP address and port might be derived
    //# from a URI, a selected alternative service ([ALTSVC]), a configured
    //# proxy, or name resolution of any of these.
    Builder::new().build(conn).await
}

/// HTTP/3 request sender
///
/// [`send_request()`] initiates a new request and will resolve when it is ready to be sent
/// to the server. Then a [`RequestStream`] will be returned to send a request body (for
/// POST, PUT methods) and receive a response. After the whole body is sent, it is necessary
/// to call [`RequestStream::finish()`] to let the server know the request transfer is complete.
/// This includes the cases where no body is sent at all.
///
/// This struct is cloneable so multiple requests can be sent concurrently.
///
/// Existing instances are atomically counted internally, so whenever all of them have been
/// dropped, the connection will be automatically closed with HTTP/3 connection error code
/// `HTTP_NO_ERROR = 0`.
///
/// # Examples
///
/// ## Sending a request with no body
///
/// ```
/// # use salvo::http3::{quic, client::*};
/// # use http::{Request, Response};
/// # use bytes::Buf;
/// # async fn doc<T,B>(mut send_request: SendRequest<T, B>) -> Result<(), Box<dyn std::error::Error>>
/// # where
/// #     T: quic::OpenStreams<B>,
/// #     B: Buf,
/// # {
/// // Prepare the HTTP request to send to the server
/// let request = Request::get("https://www.example.com/").body(())?;
///
/// // Send the request to the server
/// let mut req_stream: RequestStream<_, _> = send_request.send_request(request).await?;
/// // Don't forget to end up the request by finishing the send stream.
/// req_stream.finish().await?;
/// // Receive the response
/// let response: Response<()> = req_stream.recv_response().await?;
/// // Process the response...
/// # Ok(())
/// # }
/// # pub fn main() {}
/// ```
///
/// ## Sending a request with a body and trailers
///
/// ```
/// # use salvo::http3::{quic, client::*};
/// # use http::{Request, Response, HeaderMap};
/// # use bytes::{Buf, Bytes};
/// # async fn doc<T,B>(mut send_request: SendRequest<T, Bytes>) -> Result<(), Box<dyn std::error::Error>>
/// # where
/// #     T: quic::OpenStreams<Bytes>,
/// # {
/// // Prepare the HTTP request to send to the server
/// let request = Request::get("https://www.example.com/").body(())?;
///
/// // Send the request to the server
/// let mut req_stream = send_request.send_request(request).await?;
/// // Send some data
/// req_stream.send_data("body".into()).await?;
/// // Prepare the trailers
/// let mut trailers = HeaderMap::new();
/// trailers.insert("trailer", "value".parse()?);
/// // Send them and finish the send stream
/// req_stream.send_trailers(trailers).await?;
/// // We don't need to finish the send stream, as `send_trailers()` did it for us
///
/// // Receive the response.
/// let response = req_stream.recv_response().await?;
/// // Process the response...
/// # Ok(())
/// # }
/// # pub fn main() {}
/// ```
///
/// [`send_request()`]: struct.SendRequest.html#method.send_request
/// [`RequestStream`]: struct.RequestStream.html
/// [`RequestStream::finish()`]: struct.RequestStream.html#method.finish
pub struct SendRequest<T, B>
where
    T: quic::OpenStreams<B>,
    B: Buf,
{
    open: T,
    conn_state: SharedStateRef,
    max_field_section_size: u64, // maximum size for a header we receive
    // counts instances of SendRequest to close the connection when the last is dropped.
    sender_count: Arc<AtomicUsize>,
    conn_waker: Option<Waker>,
    _buf: PhantomData<fn(B)>,
    send_grease_frame: bool,
}

impl<T, B> SendRequest<T, B>
where
    T: quic::OpenStreams<B>,
    B: Buf,
{
    /// Send a HTTP/3 request to the server
    pub async fn send_request(&mut self, req: http::Request<()>) -> Result<RequestStream<T::BidiStream, B>, Error> {
        let (peer_max_field_section_size, closing) = {
            let state = self.conn_state.read("send request lock state");
            (state.peer_max_field_section_size, state.closing)
        };

        if closing.is_some() {
            return Err(Error::closing());
        }

        let (parts, _) = req.into_parts();
        let request::Parts {
            method, uri, headers, ..
        } = parts;
        let headers = Header::request(method, uri, headers)?;

        //= https://www.rfc-editor.org/rfc/rfc9114#section-4.1
        //= type=implication
        //# A
        //# client MUST send only a single request on a given stream.
        let mut stream = future::poll_fn(|cx| self.open.poll_open_bidi(cx))
            .await
            .map_err(|e| self.maybe_conn_err(e))?;

        //= https://www.rfc-editor.org/rfc/rfc9114#section-4.2
        //= type=TODO
        //# Characters in field names MUST be
        //# converted to lowercase prior to their encoding.

        //= https://www.rfc-editor.org/rfc/rfc9114#section-4.2.1
        //= type=TODO
        //# To allow for better compression efficiency, the Cookie header field
        //# ([COOKIES]) MAY be split into separate field lines, each with one or
        //# more cookie-pairs, before compression.

        let mut block = BytesMut::new();
        let mem_size = qpack::encode_stateless(&mut block, headers)?;

        //= https://www.rfc-editor.org/rfc/rfc9114#section-4.2.2
        //# An implementation that
        //# has received this parameter SHOULD NOT send an HTTP message header
        //# that exceeds the indicated size, as the peer will likely refuse to
        //# process it.
        if mem_size > peer_max_field_section_size {
            return Err(Error::header_too_big(mem_size, peer_max_field_section_size));
        }

        stream::write(&mut stream, Frame::Headers(block.freeze()))
            .await
            .map_err(|e| self.maybe_conn_err(e))?;

        let request_stream = RequestStream {
            inner: connection::RequestStream::new(
                FrameStream::new(stream),
                self.max_field_section_size,
                self.conn_state.clone(),
                self.send_grease_frame,
            ),
        };
        // send the grease frame only once
        self.send_grease_frame = false;
        Ok(request_stream)
    }
}

impl<T, B> ConnectionState for SendRequest<T, B>
where
    T: quic::OpenStreams<B>,
    B: Buf,
{
    fn shared_state(&self) -> &SharedStateRef {
        &self.conn_state
    }
}

impl<T, B> Clone for SendRequest<T, B>
where
    T: quic::OpenStreams<B> + Clone,
    B: Buf,
{
    fn clone(&self) -> Self {
        self.sender_count.fetch_add(1, std::sync::atomic::Ordering::Release);

        Self {
            open: self.open.clone(),
            conn_state: self.conn_state.clone(),
            max_field_section_size: self.max_field_section_size,
            sender_count: self.sender_count.clone(),
            conn_waker: self.conn_waker.clone(),
            _buf: PhantomData,
            send_grease_frame: self.send_grease_frame,
        }
    }
}

impl<T, B> Drop for SendRequest<T, B>
where
    T: quic::OpenStreams<B>,
    B: Buf,
{
    fn drop(&mut self) {
        if self.sender_count.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) == 1 {
            if let Some(w) = self.conn_waker.take() {
                w.wake()
            }
            self.shared_state().write("SendRequest drop").error = Some(Error::closed());
            self.open.close(Code::H3_NO_ERROR, b"");
        }
    }
}

/// Client connection driver
///
/// Maintains the internal state of an HTTP/3 connection, including control and QPACK.
/// It needs to be polled continuously via [`poll_close()`]. On connection closure, this
/// will resolve to `Ok(())` if the peer sent `HTTP_NO_ERROR`, or `Err()` if a connection-level
/// error occured.
///
/// [`shutdown()`] initiates a graceful shutdown of this connection. After calling it, no request
/// initiation will be further allowed. Then [`poll_close()`] will resolve when all ongoing requests
/// and push streams complete. Finally, a connection closure with `HTTP_NO_ERROR` code will be
/// sent to the server.
///
/// # Examples
///
/// ## Drive a connection concurrently
///
/// ```
/// # use bytes::Buf;
/// # use futures_util::future;
/// # use salvo::http3::{client::*, quic};
/// # use tokio::task::JoinHandle;
/// # async fn doc<C, B>(mut connection: Connection<C, B>)
/// #    -> JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>
/// # where
/// #    C: quic::Connection<B> + Send + 'static,
/// #    C::SendStream: Send + 'static,
/// #    C::RecvStream: Send + 'static,
/// #    B: Buf + Send + 'static,
/// # {
/// // Run the driver on a different task
/// tokio::spawn(async move {
///     future::poll_fn(|cx| connection.poll_close(cx)).await?;
///     Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
/// })
/// # }
/// ```
///
/// ## Shutdown a connection gracefully
///
/// ```
/// # use bytes::Buf;
/// # use futures_util::future;
/// # use salvo::http3::{client::*, quic};
/// # use tokio::{self, sync::oneshot, task::JoinHandle};
/// # async fn doc<C, B>(mut connection: Connection<C, B>)
/// #    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
/// # where
/// #    C: quic::Connection<B> + Send + 'static,
/// #    C::SendStream: Send + 'static,
/// #    C::RecvStream: Send + 'static,
/// #    B: Buf + Send + 'static,
/// # {
/// // Prepare a channel to stop the driver thread
/// let (shutdown_tx, shutdown_rx) = oneshot::channel();
///
/// // Run the driver on a different task
/// let driver = tokio::spawn(async move {
///     tokio::select! {
///         // Drive the connection
///         closed = future::poll_fn(|cx| connection.poll_close(cx)) => closed?,
///         // Listen for shutdown condition
///         max_streams = shutdown_rx => {
///             // Initiate shutdown
///             connection.shutdown(max_streams?);
///             // Wait for ongoing work to complete
///             future::poll_fn(|cx| connection.poll_close(cx)).await?;
///         }
///     };
///
///     Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
/// });
///
/// // Do client things, wait for close connection...
///
/// // Initiate shutdown
/// shutdown_tx.send(2);
/// // Wait for the connection to be closed
/// driver.await?
/// # }
/// ```
/// [`poll_close()`]: struct.Connection.html#method.poll_close
/// [`shutdown()`]: struct.Connection.html#method.shutdown
pub struct Connection<C, B>
where
    C: quic::Connection<B>,
    B: Buf,
{
    inner: ConnectionInner<C, B>,
}

impl<C, B> Connection<C, B>
where
    C: quic::Connection<B>,
    B: Buf,
{
    /// Initiate a graceful shutdown, accepting `max_request` potentially in-flight server push
    pub async fn shutdown(&mut self, max_requests: usize) -> Result<(), Error> {
        self.inner.shutdown(max_requests).await
    }

    /// Wait until the connection is closed
    pub async fn wait_idle(&mut self) -> Result<(), Error> {
        future::poll_fn(|cx| self.poll_close(cx)).await
    }

    /// Maintain the connection state until it is closed
    pub fn poll_close(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        while let Poll::Ready(result) = self.inner.poll_control(cx) {
            match result {
                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4.2
                //= type=TODO
                //# When a 0-RTT QUIC connection is being used, the initial value of each
                //# server setting is the value used in the previous session.  Clients
                //# SHOULD store the settings the server provided in the HTTP/3
                //# connection where resumption information was provided, but they MAY
                //# opt not to store settings in certain cases (e.g., if the session
                //# ticket is received before the SETTINGS frame).

                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4.2
                //= type=TODO
                //# A client MUST comply
                //# with stored settings -- or default values if no values are stored --
                //# when attempting 0-RTT.

                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4.2
                //= type=TODO
                //# Once a server has provided new settings,
                //# clients MUST comply with those values.
                Ok(Frame::Settings(_)) => trace!("Got settings"),
                Ok(Frame::Goaway(id)) => {
                    //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.6
                    //# The GOAWAY frame is always sent on the control stream.  In the
                    //# server-to-client direction, it carries a QUIC stream ID for a client-
                    //# initiated bidirectional stream encoded as a variable-length integer.
                    //# A client MUST treat receipt of a GOAWAY frame containing a stream ID
                    //# of any other type as a connection error of type H3_ID_ERROR.
                    if !id.is_request() {
                        return Poll::Ready(Err(Code::H3_ID_ERROR.with_reason(
                            format!("non-request StreamId in a GoAway frame: {}", id),
                            ErrorLevel::ConnectionError,
                        )));
                    }
                    info!("Server initiated graceful shutdown, last: StreamId({})", id);
                }

                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.5
                //# If a PUSH_PROMISE frame is received on the control stream, the client
                //# MUST respond with a connection error of type H3_FRAME_UNEXPECTED.

                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.7
                //# A client MUST treat the
                //# receipt of a MAX_PUSH_ID frame as a connection error of type
                //# H3_FRAME_UNEXPECTED.
                Ok(frame) => {
                    return Poll::Ready(Err(Code::H3_FRAME_UNEXPECTED.with_reason(
                        format!("on client control stream: {:?}", frame),
                        ErrorLevel::ConnectionError,
                    )))
                }
                Err(e) => {
                    let connection_error = self.inner.shared.read("poll_close error read").error.as_ref().cloned();

                    match connection_error {
                        Some(e) if e.is_closed() => return Poll::Ready(Ok(())),
                        Some(e) => return Poll::Ready(Err(e)),
                        None => {
                            self.inner.shared.write("poll_close error").error = e.clone().into();
                            return Poll::Ready(Err(e));
                        }
                    }
                }
            }
        }

        //= https://www.rfc-editor.org/rfc/rfc9114#section-6.1
        //# Clients MUST treat
        //# receipt of a server-initiated bidirectional stream as a connection
        //# error of type H3_STREAM_CREATION_ERROR unless such an extension has
        //# been negotiated.
        if self.inner.poll_accept_request(cx).is_ready() {
            return Poll::Ready(Err(self
                .inner
                .close(Code::H3_STREAM_CREATION_ERROR, "client received a bidirectional stream")));
        }

        Poll::Pending
    }
}

/// HTTP/3 client builder
///
/// Set the configuration for a new client.
///
/// # Examples
///
/// ```
/// # use salvo::http3::quic;
/// # async fn doc<C, O, B>(quic: C)
/// # where
/// #   C: quic::Connection<B, OpenStreams = O>,
/// #   O: quic::OpenStreams<B>,
/// #   B: bytes::Buf,
/// # {
/// let h3_conn = salvo::http3::client::builder()
///     .max_field_section_size(8192)
///     .build(quic)
///     .await
///     .expect("Failed to build connection");
/// # }
/// ```
pub struct Builder {
    max_field_section_size: u64,
    send_grease: bool,
}

impl Builder {
    pub(super) fn new() -> Self {
        Builder {
            max_field_section_size: VarInt::MAX.0,
            send_grease: true,
        }
    }

    /// Set the maximum header size this client is willing to accept
    ///
    /// See [header size constraints] section of the specification for details.
    ///
    /// [header size constraints]: https://www.rfc-editor.org/rfc/rfc9114.html#name-header-size-constraints
    pub fn max_field_section_size(&mut self, value: u64) -> &mut Self {
        self.max_field_section_size = value;
        self
    }

    /// Create a new HTTP/3 client from a `quic` connection
    pub async fn build<C, O, B>(&mut self, quic: C) -> Result<(Connection<C, B>, SendRequest<O, B>), Error>
    where
        C: quic::Connection<B, OpenStreams = O>,
        O: quic::OpenStreams<B>,
        B: Buf,
    {
        let open = quic.opener();
        let conn_state = SharedStateRef::default();

        let conn_waker = Some(future::poll_fn(|cx| Poll::Ready(cx.waker().clone())).await);

        Ok((
            Connection {
                inner: ConnectionInner::new(quic, self.max_field_section_size, conn_state.clone(), self.send_grease)
                    .await?,
            },
            SendRequest {
                open,
                conn_state,
                conn_waker,
                max_field_section_size: self.max_field_section_size,
                sender_count: Arc::new(AtomicUsize::new(1)),
                _buf: PhantomData,
                send_grease_frame: self.send_grease,
            },
        ))
    }
}

/// Manage request bodies transfer, response and trailers.
///
/// Once a request has been sent via [`send_request()`], a response can be awaited by calling
/// [`recv_response()`]. A body for this request can be sent with [`send_data()`], then the request
/// shall be completed by either sending trailers with [`send_trailers()`], or [`finish()`].
///
/// After receiving the response's headers, it's body can be read by [`recv_data()`] until it returns
/// `None`. Then the trailers will eventually be available via [`recv_trailers()`].
///
/// TODO: If data is polled before the response has been received, an error will be thrown.
///
/// TODO: If trailers are polled but the body hasn't been fully received, an UNEXPECT_FRAME error will be
/// thrown
///
/// Whenever the client wants to cancel this request, it can call [`stop_sending()`], which will
/// put an end to any transfer concerning it.
///
/// # Examples
///
/// ```
/// # use salvo::http3::{quic, client::*};
/// # use http::{Request, Response};
/// # use bytes::Buf;
/// # use tokio::io::AsyncWriteExt;
/// # async fn doc<T,B>(mut req_stream: RequestStream<T, B>) -> Result<(), Box<dyn std::error::Error>>
/// # where
/// #     T: quic::RecvStream,
/// #     B: Buf,
/// # {
/// // Prepare the HTTP request to send to the server
/// let request = Request::get("https://www.example.com/").body(())?;
///
/// // Receive the response
/// let response = req_stream.recv_response().await?;
/// // Receive the body
/// while let Some(mut chunk) = req_stream.recv_data().await? {
///     let mut out = tokio::io::stdout();
///     out.write_all_buf(&mut chunk).await?;
///     out.flush().await?;
/// }
/// # Ok(())
/// # }
/// # pub fn main() {}
/// ```
///
/// [`send_request()`]: struct.SendRequest.html#method.send_request
/// [`recv_response()`]: #method.recv_response
/// [`recv_data()`]: #method.recv_data
/// [`send_data()`]: #method.send_data
/// [`send_trailers()`]: #method.send_trailers
/// [`recv_trailers()`]: #method.recv_trailers
/// [`finish()`]: #method.finish
/// [`stop_sending()`]: #method.stop_sending
pub struct RequestStream<S, B> {
    inner: connection::RequestStream<S, B>,
}

impl<S, B> ConnectionState for RequestStream<S, B> {
    fn shared_state(&self) -> &SharedStateRef {
        &self.inner.conn_state
    }
}

impl<S, B> RequestStream<S, B>
where
    S: quic::RecvStream,
{
    /// Receive the HTTP/3 response
    ///
    /// This should be called before trying to receive any data with [`recv_data()`].
    ///
    /// [`recv_data()`]: #method.recv_data
    pub async fn recv_response(&mut self) -> Result<Response<()>, Error> {
        let mut frame = future::poll_fn(|cx| self.inner.stream.poll_next(cx))
            .await
            .map_err(|e| self.maybe_conn_err(e))?
            .ok_or_else(|| {
                Code::H3_GENERAL_PROTOCOL_ERROR
                    .with_reason("Did not receive response headers", ErrorLevel::ConnectionError)
            })?;

        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.5
        //= type=TODO
        //# A client MUST treat
        //# receipt of a PUSH_PROMISE frame that contains a larger push ID than
        //# the client has advertised as a connection error of H3_ID_ERROR.

        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.5
        //= type=TODO
        //# If a client
        //# receives a push ID that has already been promised and detects a
        //# mismatch, it MUST respond with a connection error of type
        //# H3_GENERAL_PROTOCOL_ERROR.

        let decoded = if let Frame::Headers(ref mut encoded) = frame {
            match qpack::decode_stateless(encoded, self.inner.max_field_section_size) {
                //= https://www.rfc-editor.org/rfc/rfc9114#section-4.2.2
                //# An HTTP/3 implementation MAY impose a limit on the maximum size of
                //# the message header it will accept on an individual HTTP message.
                Err(qpack::DecoderError::HeaderTooLong(cancel_size)) => {
                    self.inner.stop_sending(Code::H3_REQUEST_CANCELLED);
                    return Err(Error::header_too_big(cancel_size, self.inner.max_field_section_size));
                }
                Ok(decoded) => decoded,
                Err(e) => return Err(e.into()),
            }
        } else {
            return Err(Code::H3_FRAME_UNEXPECTED
                .with_reason("First response frame is not headers", ErrorLevel::ConnectionError));
        };

        let qpack::Decoded { fields, .. } = decoded;

        let (status, headers) = Header::try_from(fields)?.as_response_parts()?;
        let mut resp = Response::new(());
        *resp.status_mut() = status;
        *resp.headers_mut() = headers;
        *resp.version_mut() = http::Version::HTTP_3;

        Ok(resp)
    }

    /// Receive some of the request body.
    // TODO what if called before recv_response ?
    pub async fn recv_data(&mut self) -> Result<Option<impl Buf>, Error> {
        self.inner.recv_data().await
    }

    /// Receive an optional set of trailers for the response.
    pub async fn recv_trailers(&mut self) -> Result<Option<HeaderMap>, Error> {
        let res = self.inner.recv_trailers().await;
        if let Err(ref e) = res {
            if e.is_header_too_big() {
                self.inner.stream.stop_sending(Code::H3_REQUEST_CANCELLED);
            }
        }
        res
    }

    /// Tell the peer to stop sending into the underlying QUIC stream
    pub fn stop_sending(&mut self, error_code: crate::error::Code) {
        // TODO take by value to prevent any further call as this request is cancelled
        // rename `cancel()` ?
        self.inner.stream.stop_sending(error_code)
    }
}

impl<S, B> RequestStream<S, B>
where
    S: quic::SendStream<B>,
    B: Buf,
{
    /// Send some data on the request body.
    pub async fn send_data(&mut self, buf: B) -> Result<(), Error> {
        self.inner.send_data(buf).await
    }

    /// Send a set of trailers to end the request.
    ///
    /// Either [`RequestStream::finish`] or
    /// [`RequestStream::send_trailers`] must be called to finalize a
    /// request.
    pub async fn send_trailers(&mut self, trailers: HeaderMap) -> Result<(), Error> {
        self.inner.send_trailers(trailers).await
    }

    /// End the request without trailers.
    ///
    /// Either [`RequestStream::finish`] or
    /// [`RequestStream::send_trailers`] must be called to finalize a
    /// request.
    pub async fn finish(&mut self) -> Result<(), Error> {
        self.inner.finish().await
    }

    //= https://www.rfc-editor.org/rfc/rfc9114#section-4.1.1
    //= type=TODO
    //# Implementations SHOULD cancel requests by abruptly terminating any
    //# directions of a stream that are still open.  To do so, an
    //# implementation resets the sending parts of streams and aborts reading
    //# on the receiving parts of streams; see Section 2.4 of
    //# [QUIC-TRANSPORT].
}

impl<S, B> RequestStream<S, B>
where
    S: quic::BidiStream<B>,
    B: Buf,
{
    /// Split this stream into two halves that can be driven independently.
    pub fn split(self) -> (RequestStream<S::SendStream, B>, RequestStream<S::RecvStream, B>) {
        let (send, recv) = self.inner.split();
        (RequestStream { inner: send }, RequestStream { inner: recv })
    }
}
