use futures_util::TryStreamExt;
use hyper::upgrade::OnUpgrade;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::{connect::HttpConnector, Client as HyperUtilClient};
use hyper_util::rt::TokioExecutor;
use salvo_core::http::header::{HeaderMap, HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use salvo_core::http::uri::{Scheme, Uri};
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::rt::tokio::TokioIo;
use salvo_core::{async_trait, BoxedError, Depot, Error, FlowCtrl, Handler, Request, Response};
use tokio::io::copy_bidirectional;

use super::{HyperRequest, HyperResponse};

pub struct HyperClient {
    inner: HyperUtilClient<HttpsConnector<HttpConnector>, ReqBody>,
}
impl Default for HyperClient {
    fn default() -> Self {
        Self {
            inner: HyperUtilClient::builder(TokioExecutor::new()).build(HttpsConnector::new()),
        }
    }
}
impl HyperClient {
    pub fn new(inner: HyperUtilClient<HttpsConnector<HttpConnector>, ReqBody>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl super::Client for HyperClient {
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
            println!("=======response_upgrade_type {:?}", response_upgrade_type);

            if request_upgrade_type.as_deref() == response_upgrade_type {
                // let mut hyper_response = hyper_response.body(ResBody::None).map_err(Error::other)?;
                println!("--------------------0");
                let response_upgraded = hyper::upgrade::on(&mut response).await.unwrap();
                println!("--------------------1");
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(request_upgraded) => {
                                println!("--------------------2");
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
        
        Ok(response.map(|b|ResBody::Hyper(b)))
    }
}

//TODO: ReqwestClient
