use futures_util::TryStreamExt;
use hyper::upgrade::OnUpgrade;
use reqwest::Client as InnerClient;
use salvo_core::Error;
use salvo_core::http::{ResBody, StatusCode};
use salvo_core::rt::tokio::TokioIo;
use tokio::io::copy_bidirectional;

use crate::{BoxedError, Client, HyperRequest, HyperResponse, Proxy, Upstreams};

/// A [`Client`] implementation based on [`reqwest::Client`].
///
/// This client provides proxy capabilities using the Reqwest HTTP client.
/// Redirect following is disabled by default for proxy safety. If you need to
/// follow upstream redirects, pass a custom [`reqwest::Client`] to [`ReqwestClient::new`].
#[derive(Clone, Debug)]
pub struct ReqwestClient {
    inner: InnerClient,
}

impl<U> Proxy<U, ReqwestClient>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
{
    /// Create a new `Proxy` using the default Reqwest client.
    ///
    /// This is a convenient way to create a proxy with standard configuration.
    pub fn use_reqwest_client(upstreams: U) -> Self {
        Self::new(upstreams, ReqwestClient::default())
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        #[cfg(feature = "ring")]
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self::new(
            InnerClient::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build reqwest client"),
        )
    }
}

impl ReqwestClient {
    /// Create a new `ReqwestClient` with the given [`reqwest::Client`].
    #[must_use]
    pub fn new(inner: InnerClient) -> Self {
        Self { inner }
    }
}

impl Client for ReqwestClient {
    type Error = salvo_core::Error;

    async fn execute(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse, Self::Error> {
        let request_upgrade_type = crate::get_upgrade_type(proxied_request.headers());

        let proxied_request = proxied_request.map(|body| {
            // Forward only data frames; drop non-data frames (e.g. trailers, which
            // reqwest cannot send anyway) instead of turning them into empty bytes,
            // and propagate stream errors so a failed/aborted upstream body surfaces
            // as an error rather than being silently truncated into a "successful" body.
            reqwest::Body::wrap_stream(
                body.try_filter_map(|frame| std::future::ready(Ok(frame.into_data().ok()))),
            )
        });
        let mut response = self
            .inner
            .execute(proxied_request.try_into().map_err(Error::other)?)
            .await
            .map_err(Error::other)?;

        let status = response.status();
        let version = response.version();
        let response_upgrade_type = if status == StatusCode::SWITCHING_PROTOCOLS {
            crate::get_upgrade_type(response.headers())
        } else {
            None
        };
        let res_headers = std::mem::take(response.headers_mut());
        let hyper_response = hyper::Response::builder()
            .status(status)
            .version(version);

        let mut hyper_response = if status == StatusCode::SWITCHING_PROTOCOLS {
            if crate::upgrade_types_match(
                request_upgrade_type.as_deref(),
                response_upgrade_type.as_deref(),
            ) {
                let mut response_upgraded = response.upgrade().await.map_err(|e| {
                    Error::other(format!("response does not have an upgrade extension. {e}"))
                })?;
                if let Some(request_upgraded) = request_upgraded {
                    tokio::spawn(async move {
                        match request_upgraded.await {
                            Ok(request_upgraded) => {
                                let mut request_upgraded = TokioIo::new(request_upgraded);
                                if let Err(e) = copy_bidirectional(
                                    &mut response_upgraded,
                                    &mut request_upgraded,
                                )
                                .await
                                {
                                    tracing::error!(error = ?e, "copying between upgraded connections failed");
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

// Unit tests for Proxy
#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::*;

    use super::*;
    use crate::{Proxy, Upstreams};

    #[tokio::test]
    async fn test_upstreams_elect() {
        let upstreams = vec!["https://www.example.com", "https://www.example2.com"];
        let proxy = Proxy::new(upstreams.clone(), ReqwestClient::default());
        let request = Request::new();
        let depot = Depot::new();
        let elected_upstream = proxy.upstreams().elect(&request, &depot).await.unwrap();
        assert!(upstreams.contains(&elected_upstream));
    }

    #[tokio::test]
    async fn test_reqwest_client() {
        #[cfg(feature = "ring")]
        let _ = rustls::crypto::ring::default_provider().install_default();
        // Ignore environment proxies so even an all-features run stays local.
        let client = ReqwestClient::new(InnerClient::builder().no_proxy().build().unwrap());
        struct UpstreamTask(tokio::task::JoinHandle<()>);
        impl Drop for UpstreamTask {
            fn drop(&mut self) {
                self.0.abort();
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut upstream = UpstreamTask(tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let service = hyper::service::service_fn(
                |req: hyper::Request<hyper::body::Incoming>| async move {
                    assert_eq!(req.method(), hyper::Method::GET);
                    assert_eq!(req.uri().path(), "/guide/index.html");
                    assert_eq!(req.uri().query(), Some("lang=en"));
                    Ok::<_, std::convert::Infallible>(
                        hyper::Response::builder()
                            .status(StatusCode::CREATED)
                            .header("content-type", "text/plain")
                            .header("x-test-upstream", "local-reqwest")
                            .body(ResBody::from("controlled reqwest upstream body"))
                            .unwrap(),
                    )
                },
            );
            hyper::server::conn::http1::Builder::new()
                .keep_alive(false)
                .serve_connection(TokioIo::new(stream), service)
                .await
                .unwrap();
        }));
        let router = Router::new().push(
            Router::with_path("rust/{**rest}")
                .goal(Proxy::new(vec![format!("http://{address}")], client)),
        );

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut response =
                TestClient::get("http://local.example/rust/guide/index.html?lang=en")
                    .send(router)
                    .await;
            assert_eq!(response.status_code, Some(StatusCode::CREATED));
            assert_eq!(response.headers()["x-test-upstream"], "local-reqwest");
            assert_eq!(
                response.take_string().await.unwrap(),
                "controlled reqwest upstream body"
            );
        })
        .await
        .expect("local proxy round trip timed out");
        tokio::time::timeout(std::time::Duration::from_secs(10), &mut upstream.0)
            .await
            .expect("local upstream did not finish")
            .unwrap();
    }

    #[test]
    fn test_others() {
        let mut handler = Proxy::new(["https://www.bing.com"], ReqwestClient::default());
        assert_eq!(handler.upstreams().len(), 1);
        assert_eq!(handler.upstreams_mut().len(), 1);
    }
}
