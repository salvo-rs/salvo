//! HTTP3 suppports.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Mutex;

use bytes::Bytes;
use futures_util::future::poll_fn;
use futures_util::Stream;
use h3::error::ErrorLevel;
use h3::ext::Protocol;
use h3::server::RequestStream;

use crate::proto::WebTransportSession;
use crate::http::body::{H3ReqBody, ReqBody};
use crate::http::Method;

/// Builder is used to serve HTTP3 connection.
pub struct Builder(h3::server::Builder);
impl Deref for Builder {
    type Target = h3::server::Builder;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Builder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
impl Builder {
    /// Create a new builder.
    pub fn new() -> Self {
        let mut builder = h3::server::builder();
        builder
            .enable_webtransport(true)
            .enable_connect(true)
            .enable_datagram(true)
            .max_webtransport_sessions(1)
            .send_grease(true);
        Self(builder)
    }
}

impl Builder {
    /// Serve HTTP3 connection.
    pub async fn serve_connection(
        &self,
        conn: crate::conn::quinn::H3Connection,
        hyper_handler: crate::service::HyperHandler,
    ) -> IoResult<()> {
        let mut conn = self
            .0
            .build::<h3_quinn::Connection, bytes::Bytes>(conn.into_inner())
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, format!("invalid connection: {}", e)))?;
        loop {
            match conn.accept().await {
                Ok(Some((request, stream))) => {
                    tracing::debug!("new request: {:#?}", request);
                    let hyper_handler = hyper_handler.clone();
                    match request.method() {
                        &Method::CONNECT
                            if request.extensions().get::<Protocol>() == Some(&Protocol::WEB_TRANSPORT) =>
                        {
                            if let Some(c) = process_web_transport(conn, request, stream, hyper_handler).await? {
                                conn = c;
                            } else {
                                return Ok(());
                            }
                        }
                        _ => {
                            tokio::spawn(async move {
                                match process_request(request, stream, hyper_handler).await {
                                    Ok(_) => {}
                                    Err(e) => {
                                        tracing::error!(error = ?e, "process request failed")
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
                    tracing::warn!(error = ?e, "accept failed");
                    match e.get_error_level() {
                        ErrorLevel::ConnectionError => break,
                        ErrorLevel::StreamError => continue,
                    }
                }
            }
        }
        Ok(())
    }
}

async fn process_web_transport(
    conn: h3::server::Connection<h3_quinn::Connection, Bytes>,
    request: hyper::Request<()>,
    stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    mut hyper_handler: crate::service::HyperHandler,
) -> IoResult<Option<h3::server::Connection<h3_quinn::Connection, Bytes>>> {
    let (parts, _body) = request.into_parts();
    let mut request = hyper::Request::from_parts(parts, ReqBody::None);
    request.extensions_mut().insert(Mutex::new(conn));
    request.extensions_mut().insert(stream);

    let mut response = hyper::service::Service::call(&mut hyper_handler, request)
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to call hyper service : {}", e)))?;

    let conn;
    let stream;
    if let Some(session) = response
        .extensions_mut()
        .remove::<WebTransportSession<h3_quinn::Connection, Bytes>>()
    {
        let WebTransportSession {
            server_conn,
            connect_stream,
            ..
        } = session;

        conn = Some(
            server_conn
                .into_inner()
                .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to get conn : {}", e)))?,
        );
        stream = Some(connect_stream);
    } else {
        conn = response
            .extensions_mut()
            .remove::<Mutex<h3::server::Connection<h3_quinn::Connection, Bytes>>>()
            .map(|c| {
                c.into_inner()
                    .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to get conn : {}", e)))
            })
            .transpose()?;
        stream = response
            .extensions_mut()
            .remove::<h3::server::RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>>();
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
            Ok(bytes) => {
                if let Err(e) = stream.send_data(bytes).await {
                    tracing::error!(error = ?e, "unable to send data to connection peer");
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
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to finish stream : {}", e)))?;

    Ok(Some(conn))
}

#[allow(clippy::future_not_send)]
async fn process_request<S>(
    request: hyper::Request<()>,
    stream: RequestStream<S, Bytes>,
    mut hyper_handler: crate::service::HyperHandler,
) -> IoResult<()>
where
    S: h3::quic::BidiStream<Bytes> + Send + Unpin + 'static,
    <S as h3::quic::BidiStream<Bytes>>::RecvStream: Send + Sync + Unpin,
{
    let (mut tx, rx) = stream.split();
    let (parts, _body) = request.into_parts();
    let request = hyper::Request::from_parts(parts, ReqBody::from(H3ReqBody::new(rx)));

    let response = hyper::service::Service::call(&mut hyper_handler, request)
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to call hyper service : {}", e)))?;

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
            Ok(bytes) => {
                if let Err(e) = tx.send_data(bytes).await {
                    tracing::error!(error = ?e, "unable to send data to connection peer");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "unable to poll data from connection");
            }
        }
    }
    tx.finish()
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to finish stream : {}", e)))
}
