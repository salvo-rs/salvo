//! HTTP3 suppports.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use bytes::Bytes;
use futures_util::future::poll_fn;
use futures_util::Stream;
use salvo_http3::error::ErrorLevel;
use salvo_http3::ext::Protocol;
use salvo_http3::server::RequestStream;
use tokio_util::sync::CancellationToken;

use crate::http::body::{H3ReqBody, ReqBody};
use crate::http::Method;
use crate::proto::WebTransportSession;

/// Builder is used to serve HTTP3 connection.
pub struct Builder(salvo_http3::server::Builder);
impl Deref for Builder {
    type Target = salvo_http3::server::Builder;
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
        let mut builder = salvo_http3::server::builder();
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
        _server_shutdown_token: CancellationToken, //TODO
        _idle_connection_timeout: Option<Duration>, //TODO
    ) -> IoResult<()> {
        let mut conn = self
            .0
            .build::<salvo_http3::http3_quinn::Connection, bytes::Bytes>(conn.into_inner())
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
    conn: salvo_http3::server::Connection<salvo_http3::http3_quinn::Connection, Bytes>,
    request: hyper::Request<()>,
    stream: RequestStream<salvo_http3::http3_quinn::BidiStream<Bytes>, Bytes>,
    hyper_handler: crate::service::HyperHandler,
) -> IoResult<Option<salvo_http3::server::Connection<salvo_http3::http3_quinn::Connection, Bytes>>> {
    let (parts, _body) = request.into_parts();
    let mut request = hyper::Request::from_parts(parts, ReqBody::None);
    request.extensions_mut().insert(Mutex::new(conn));
    request.extensions_mut().insert(stream);

    let mut response = hyper::service::Service::call(&hyper_handler, request)
        .await
        .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to call hyper service : {}", e)))?;

    let conn;
    let stream;
    if let Some(session) = response
        .extensions_mut()
        .remove::<WebTransportSession<salvo_http3::http3_quinn::Connection, Bytes>>()
    {
        let (server_conn, connect_stream) = session.split();

        conn = Some(
            server_conn
                .into_inner()
                .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to get conn : {}", e)))?,
        );
        stream = Some(connect_stream);
    } else {
        conn = response
            .extensions_mut()
            .remove::<Mutex<salvo_http3::server::Connection<salvo_http3::http3_quinn::Connection, Bytes>>>()
            .map(|c| {
                c.into_inner()
                    .map_err(|e| IoError::new(ErrorKind::Other, format!("failed to get conn : {}", e)))
            })
            .transpose()?;
        stream = response
            .extensions_mut()
            .remove::<salvo_http3::server::RequestStream<salvo_http3::http3_quinn::BidiStream<Bytes>, Bytes>>();
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
