//! Proxy support for Savlo web server framework.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::convert::{Infallible, TryFrom};

use futures_util::TryStreamExt;
use hyper::upgrade::OnUpgrade;
use hyper_util::client::legacy::{connect::HttpConnector, Client as HyperUtilClient};
use hyper_util::rt::TokioExecutor;
use percent_encoding::{utf8_percent_encode, CONTROLS};
use reqwest::Client;
use salvo_core::http::header::{HeaderMap, HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use salvo_core::http::uri::Uri;
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::rt::tokio::TokioIo;
use salvo_core::{async_trait, BoxedError, Depot, Error, FlowCtrl, Handler, Request, Response};
use tokio::io::copy_bidirectional;

use super::{HyperRequest, HyperResponse};

pub struct HyperClient {
    http_client: HyperUtilClient<TokioExecutor, ReqBody>,
    https_client: HyperUtilClient<TokioExecutor, ReqBody>,
}
impl HyperClient {
    pub fn new() -> Self {
        Self {
            http_client: HyperUtilClient::builder(TokioExecutor::new()).build(HttpConnector::new()),
            https_client: HyperUtilClient::builder(TokioExecutor::new()).build(HttpConnector::new()),
        }
    }
    pub fn custom(
        http_client: HyperUtilClient<TokioExecutor, ReqBody>,
        https_client: HyperUtilClient<TokioExecutor, ReqBody>,
    ) -> Self {
        Self {
            http_client,
            https_client,
        }
    }
}

#[async_trait]
impl super::Client for HyperClient {
    type Error = hyper::Error;

    async fn execute(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse, Self::Error> {
        let request_upgrade_type = crate::get_upgrade_type(proxied_request.headers()).map(|s| s.to_owned());

        let proxied_request =
            proxied_request.map(|s| reqwest::Body::wrap_stream(s.map_ok(|s| s.into_data().unwrap_or_default())));
        let response: ! = self.http_client
            .execute(proxied_request.try_into().map_err(Error::other)?)
            .await
            .map_err(Error::other)?;

        let res_headers = response.headers().clone();
        let hyper_response = hyper::Response::builder()
            .status(response.status())
            .version(response.version());

        let mut hyper_response = if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let response_upgrade_type = crate::get_upgrade_type(response.headers());

            if request_upgrade_type.as_deref() == response_upgrade_type {
                let mut response_upgraded = response
                    .upgrade()
                    .await
                    .map_err(|e| Error::other(format!("response does not have an upgrade extension. {}", e)))?;
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(request_upgraded) => {
                                let mut request_upgraded = TokioIo::new(request_upgraded);
                                if let Err(e) = copy_bidirectional(&mut response_upgraded, &mut request_upgraded).await
                                {
                                    tracing::error!(error = ?e, "coping between upgraded connections failed");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = ?e, "upgrade request failed");
                            }
                        }
                    });
                } else {
                    return Err(Error::other("request does not have an upgrade extension"));
                }
            } else {
                return Err(Error::other("upgrade type mismatch"));
            }
            hyper_response.body(ResBody::None).map_err(Error::other)?
        } else {
            hyper_response
                .body(ResBody::stream(response.bytes_stream()))
                .map_err(Error::other)?
        };
        *hyper_response.headers_mut() = res_headers;
        Ok(hyper_response)
    }
}
