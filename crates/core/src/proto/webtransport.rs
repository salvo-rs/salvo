//! WebTransport supports.

use std::sync::Mutex;

use bytes::Buf;
use futures_util::future::poll_fn;
use h3::connection::ConnectionState;
use h3::error::{Code, Error, ErrorLevel};
use h3::frame::FrameStream;
use h3::proto::frame::Frame;
use h3::quic::{self, SendDatagramExt};
use h3::server::{Connection, RequestStream};
use h3::stream::BufRecvStream;
pub use h3::webtransport::SessionId;
use h3_webtransport::server::{AcceptUni, AcceptedBi, OpenBi, OpenUni, ReadDatagram};
use h3_webtransport::stream::BidiStream;
pub use h3_webtransport::{server, stream};
use http::{Response, StatusCode};

/// A WebTransport session.
pub struct WebTransportSession<C, B>
where
    C: quic::Connection<B>,
    B: Buf,
{
    // See: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-http3/#section-2-3
    session_id: SessionId,
    /// The underlying HTTP/3 connection
    pub(crate) server_conn: Mutex<Connection<C, B>>,
    pub(crate) connect_stream: RequestStream<C::BidiStream, B>,
    opener: Mutex<C::OpenStreams>,
}

impl<C, B> WebTransportSession<C, B>
where
    C: quic::Connection<B>,
    B: Buf,
{
    /// Accepts a *CONNECT* request for establishing a WebTransport session.
    ///
    /// TODO: is the API or the user responsible for validating the CONNECT request?
    pub async fn accept(
        mut stream: RequestStream<C::BidiStream, B>,
        mut conn: Connection<C, B>,
    ) -> Result<Self, Error> {
        let shared = conn.shared_state().clone();
        {
            let config = shared.write("Read WebTransport support").peer_config;

            if !config.enable_webtransport() {
                return Err(conn.close(Code::H3_SETTINGS_ERROR, "webtransport is not supported by client"));
            }

            if !config.enable_datagram() {
                return Err(conn.close(Code::H3_SETTINGS_ERROR, "datagrams are not supported by client"));
            }
        }

        // The peer is responsible for validating our side of the webtransport support.
        //
        // However, it is still advantageous to show a log on the server as (attempting) to
        // establish a WebTransportSession without the proper h3 config is usually a mistake.
        if !conn.inner.config.enable_webtransport() {
            tracing::warn!("Server does not support webtransport");
        }

        if !conn.inner.config.enable_datagram() {
            tracing::warn!("Server does not support datagrams");
        }

        if !conn.inner.config.enable_extended_connect() {
            tracing::warn!("Server does not support CONNECT");
        }

        // Respond to the CONNECT request.

        //= https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-http3/#section-3.3
        let response = Response::builder()
            // This is the only header that chrome cares about.
            .header("sec-webtransport-http3-draft", "draft02")
            .status(StatusCode::OK)
            .body(())
            .unwrap();

        stream.send_response(response).await?;

        let session_id = stream.send_id().into();
        let conn_inner = &mut conn.inner.conn;
        let opener = Mutex::new(conn_inner.opener());

        Ok(Self {
            session_id,
            opener,
            server_conn: Mutex::new(conn),
            connect_stream: stream,
        })
    }

    /// Receive a datagram from the client
    pub fn accept_datagram(&self) -> ReadDatagram<C, B> {
        ReadDatagram::new(&self.server_conn)
    }

    /// Sends a datagram
    ///
    /// TODO: maybe make async. `quinn` does not require an async send
    pub fn send_datagram(&self, data: B) -> Result<(), Error>
    where
        C: SendDatagramExt<B>,
    {
        self.server_conn
            .lock()
            .unwrap()
            .send_datagram(self.connect_stream.id(), data)?;

        Ok(())
    }

    /// Accept an incoming unidirectional stream from the client, it reads the stream until EOF.
    pub fn accept_uni(&self) -> AcceptUni<C, B> {
        AcceptUni::new(&self.server_conn)
    }

    /// Accepts an incoming bidirectional stream or request
    pub async fn accept_bi(&self) -> Result<Option<AcceptedBi<C, B>>, Error> {
        // Get the next stream
        // Accept the incoming stream
        let stream = poll_fn(|cx| {
            let mut conn = self.server_conn.lock().unwrap();
            conn.poll_accept_request(cx)
        })
        .await;

        let mut stream = match stream {
            Ok(Some(s)) => FrameStream::new(BufRecvStream::new(s)),
            Ok(None) => {
                // FIXME: is proper HTTP GoAway shutdown required?
                return Ok(None);
            }
            Err(err) => {
                match err.kind() {
                    h3::error::Kind::Closed => return Ok(None),
                    h3::error::Kind::Application {
                        code,
                        reason,
                        level: ErrorLevel::ConnectionError,
                        ..
                    } => {
                        return Err(self
                            .server_conn
                            .lock()
                            .unwrap()
                            .close(code, reason.unwrap_or_else(|| String::into_boxed_str(String::from("")))))
                    }
                    _ => return Err(err),
                };
            }
        };

        // Read the first frame.
        //
        // This will determine if it is a webtransport bi-stream or a request stream
        let frame = poll_fn(|cx| stream.poll_next(cx)).await;

        match frame {
            Ok(None) => Ok(None),
            Ok(Some(Frame::WebTransportStream(session_id))) => {
                // Take the stream out of the framed reader and split it in half like Paul Allen
                let stream = stream.into_inner();

                Ok(Some(AcceptedBi::BidiStream(session_id, BidiStream::new(stream))))
            }
            // Make the underlying HTTP/3 connection handle the rest
            frame => {
                let req = {
                    let mut conn = self.server_conn.lock().unwrap();
                    conn.accept_with_frame(stream, frame)?
                };
                if let Some(req) = req {
                    let (req, resp) = req.resolve().await?;
                    Ok(Some(AcceptedBi::Request(req, resp)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Open a new bidirectional stream
    pub fn open_bi(&self, session_id: SessionId) -> OpenBi<C, B> {
        OpenBi::new(&self.opener, session_id)
    }

    /// Open a new unidirectional stream
    pub fn open_uni(&self, session_id: SessionId) -> OpenUni<C, B> {
        OpenUni::new(&self.opener, session_id)
    }

    /// Returns the session id
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
}
