//! HTTP3 suppports.
use std::pin::Pin;
use std::io::Result as IoResult;

use futures_util::future::poll_fn;
use futures_util::Stream;
use h3::error::ErrorLevel;

use crate::http::body::{H3ReqBody, ReqBody};

/// Builder is used to serve HTTP3 connection.
pub struct Builder;
impl Builder {
    /// Serve HTTP3 connection.
    pub async fn serve_connection(
        &self,
        mut conn: crate::conn::quinn::H3Connection,
        hyper_handler: crate::service::HyperHandler,
    ) -> IoResult<()> {
        loop {
            match conn.accept().await {
                Ok(Some((request, stream))) => {
                    tracing::debug!("new request: {:#?}", request);
                    let mut hyper_handler = hyper_handler.clone();
                    tokio::spawn(async move {
                        let (parts, _body) = request.into_parts();
                        let (mut tx, rx) = stream.split();
                        let request = hyper::Request::from_parts(parts, ReqBody::from(H3ReqBody::new(rx)));
                        let response = match hyper::service::Service::call(&mut hyper_handler, request).await {
                            Ok(response) => response,
                            Err(e) => {
                                tracing::debug!(error = ?e, "service call failed");
                                return;
                            }
                        };

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
                        if let Err(e) = tx.finish().await {
                            tracing::error!(error = ?e, "unable to finish stream");
                        }
                    });
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
