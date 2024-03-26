use futures_util::StreamExt;
use hyper::upgrade::OnUpgrade;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{connect::HttpConnector, Client as HyperUtilClient};
use hyper_util::rt::TokioExecutor;
use reqwest::Client as ReqwestUtilClient;
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::rt::tokio::TokioIo;
use salvo_core::Error;
use tokio::io::copy_bidirectional;

use super::{Client, HyperRequest, HyperResponse};

/// A [`Client`] implementation based on [`hyper_util::client::legacy::Client`].
pub struct HyperClient {
    inner: HyperUtilClient<HttpsConnector<HttpConnector>, ReqBody>,
}
impl Default for HyperClient {
    fn default() -> Self {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("no native root CA certificates found")
            .https_only()
            .enable_http1()
            .build();
        Self {
            inner: HyperUtilClient::builder(TokioExecutor::new()).build(https),
        }
    }
}
impl HyperClient {
    /// Create a new `HyperClient` with the given `HyperClient`.
    pub fn new(inner: HyperUtilClient<HttpsConnector<HttpConnector>, ReqBody>) -> Self {
        Self { inner }
    }
}

impl Client for HyperClient {
    type Error = salvo_core::Error;

    async fn execute(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse, Self::Error> {
        let request_upgrade_type = crate::get_upgrade_type(proxied_request.headers()).map(|s| s.to_owned());

        let mut response = self.inner.request(proxied_request).await.map_err(Error::other)?;

        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let response_upgrade_type = crate::get_upgrade_type(response.headers());
            if request_upgrade_type.as_deref() == response_upgrade_type {
                let response_upgraded = hyper::upgrade::on(&mut response).await?;
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(request_upgraded) => {
                                let mut request_upgraded = TokioIo::new(request_upgraded);
                                let mut response_upgraded = TokioIo::new(response_upgraded);
                                if let Err(e) = copy_bidirectional(&mut response_upgraded, &mut request_upgraded).await
                                {
                                    tracing::error!(error = ?e, "coping between upgraded connections failed.");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = ?e, "upgrade request failed.");
                            }
                        }
                    });
                } else {
                    return Err(Error::other("request does not have an upgrade extension."));
                }
            } else {
                return Err(Error::other("upgrade type mismatch"));
            }
        }
        Ok(response.map(ResBody::Hyper))
    }
}

/// A [`Client`] implementation based on [`reqwest::Client`].
pub struct ReqwestClient {
    inner: ReqwestUtilClient,
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self {
            inner: ReqwestUtilClient::default(),
        }
    }
}

impl ReqwestClient {
    fn new(inner: ReqwestUtilClient) -> Self {
        Self { inner }
    }
}

impl Client for ReqwestClient {
    type Error = salvo_core::Error;

    async fn execute(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> impl futures_util::Future<Output = Result<HyperResponse, Self::Error>> + Send {
        let request_upgrade_type = crate::get_upgrade_type(proxied_request.headers()).map(|s| s.to_owned());

        let url = proxied_request.uri().to_string();
        let method = proxied_request.method().clone();
        let headers = proxied_request.headers().clone();
        let req_body = proxied_request.into_body();
        let body = req_body.collect().await;
        // let body = reqwest::Body::from(Bytes::from(proxied_request.into_body()));
        // let incoming = hyper::body::Incoming::from(req_body);
        // incoming.channel();
        // let body = match proxied_request.into_body() {
        //     ReqBody::None => reqwest::Body::default(),
        //     ReqBody::Once(bytes) => reqwest::Body::from(bytes),
        //     ReqBody::Hyper { inner, fusewire } => {
        //         // TODO
        //         inner.
        //     }
        //     ReqBody::Boxed { inner, fusewire } => {
        //         // TODO
        //     },
        //     _ => reqwest::Body::default(),
        // };
        let req = self
            .inner
            .request(method, url)
            .headers(headers)
            .body(body)
            .build()
            .map_err(Error::other)?;
        let mut response = self.inner.execute(req).await.map_err(Error::other)?;

        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let response_upgrade_type = crate::get_upgrade_type(response.headers());
            if request_upgrade_type.as_deref() == response_upgrade_type {
                let response_upgraded = response.upgrade().await?;
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(request_upgraded) => {
                                let mut request_upgraded = TokioIo::new(request_upgraded);
                                let mut response_upgraded = TokioIo::new(response_upgraded);
                                if let Err(e) = copy_bidirectional(&mut response_upgraded, &mut request_upgraded).await
                                {
                                    tracing::error!(error = ?e, "coping between upgraded connections failed.");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = ?e, "upgrade request failed.");
                            }
                        }
                    });
                } else {
                    return Err(Error::other("request does not have an upgrade extension."));
                }
            } else {
                return Err(Error::other("upgrade type mismatch"));
            }
        }
        Ok(response.map(ResBody::Hyper))
    }
}
