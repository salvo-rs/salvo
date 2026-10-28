//! Adapters for [`tower::Layer`](https://docs.rs/tower/latest/tower/trait.Layer.html) and
//! [`tower::Service`](https://docs.rs/tower/latest/tower/trait.Service.html).
//!
//! For an MCP server built with the official Rust SDK (`rmcp`), see the
//! [Streamable HTTP example](https://github.com/salvo-rs/salvo/tree/main/examples/mcp).
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::tower_compat::*;
//! use tokio::time::Duration;
//! use tower::limit::RateLimitLayer;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let limit = RateLimitLayer::new(5, Duration::from_secs(30)).compat();
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     let router = Router::new().hoop(limit).get(hello);
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::Error as IoError;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures_util::future::{BoxFuture, FutureExt, ready};
use http_body_util::BodyExt;
use hyper::body::{Body, Bytes};
use salvo_core::http::{ReqBody, ResBody, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, async_trait, hyper};
use tokio::sync::oneshot;
use tower::buffer::Buffer;
use tower::{Layer, Service, ServiceExt};

/// Trait for tower service compat.
pub trait TowerServiceCompat<QB, SB, E, Fut> {
    /// Converts a tower service to a salvo handler.
    fn compat(self) -> TowerServiceHandler<Self, QB>
    where
        Self: Sized,
    {
        TowerServiceHandler(self, PhantomData)
    }
}

impl<T, QB, SB, E, Fut> TowerServiceCompat<QB, SB, E, Fut> for T
where
    QB: From<ReqBody> + Send + Sync + 'static,
    SB: Body + Send + Sync + 'static,
    SB::Data: Into<Bytes> + Send + fmt::Debug + 'static,
    SB::Error: StdError + Send + Sync + 'static,
    E: StdError + Send + Sync + 'static,
    T: Service<hyper::Request<ReqBody>, Response = hyper::Response<SB>, Future = Fut>
        + Clone
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<hyper::Response<SB>, E>> + Send + 'static,
{
}

/// Tower service compat handler.
pub struct TowerServiceHandler<Svc, QB>(Svc, PhantomData<QB>);
impl<Svc, QB> Debug for TowerServiceHandler<Svc, QB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TowerServiceHandler").finish()
    }
}

#[async_trait]
impl<Svc, QB, SB, E, Fut> Handler for TowerServiceHandler<Svc, QB>
where
    QB: TryFrom<ReqBody> + Body + Send + Sync + 'static,
    <QB as TryFrom<ReqBody>>::Error: StdError + Send + Sync + 'static,
    SB: Body + Send + Sync + 'static,
    SB::Data: Into<Bytes> + Send + fmt::Debug + 'static,
    SB::Error: StdError + Send + Sync + 'static,
    E: StdError + Send + Sync + 'static,
    Svc: Service<hyper::Request<QB>, Response = hyper::Response<SB>, Future = Fut>
        + Send
        + Sync
        + Clone
        + 'static,
    Svc::Error: StdError + Send + Sync + 'static,
    Fut: Future<Output = Result<hyper::Response<SB>, E>> + Send + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        let mut svc = self.0.clone();
        if svc.ready().await.is_err() {
            tracing::error!("tower service not ready");
            res.render(StatusError::internal_server_error().cause("tower service not ready"));
            return;
        }
        let Ok(hyper_req) = req.strip_to_hyper::<QB>() else {
            tracing::error!("strip request to hyper failed");
            res.render(StatusError::internal_server_error().cause("strip request to hyper failed"));
            return;
        };

        let hyper_res = match svc.call(hyper_req).await {
            Ok(hyper_res) => hyper_res,
            Err(e) => {
                tracing::error!(error = ?e, "call tower service failed: {e}");
                res.render(
                    StatusError::internal_server_error()
                        .cause(format!("call tower service failed: {e}")),
                );
                return;
            }
        }
        .map(|res| {
            ResBody::Boxed(Box::pin(
                res.map_frame(|f| f.map_data(|data| data.into()))
                    .map_err(|e| e.into()),
            ))
        });

        res.merge_hyper(hyper_res);
    }
}

struct FlowCall {
    request: hyper::Request<ReqBody>,
    response: oneshot::Sender<hyper::Response<ResBody>>,
    span: tracing::Span,
}

type FlowCallSender = Arc<Mutex<Option<oneshot::Sender<FlowCall>>>>;

const REPRESENTATION_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-encoding",
    "content-range",
    "content-disposition",
    "etag",
    "last-modified",
    "digest",
    "content-digest",
    "repr-digest",
    "transfer-encoding",
    "trailer",
];

// Request extensions may be cloned or retained by Tower. The sender is a
// single-use capability, and closing it also invalidates those retained copies.
struct CloseFlowCall(FlowCallSender);

impl Drop for CloseFlowCall {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

async fn drive_inner_call(
    call: oneshot::Receiver<FlowCall>,
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
    response_sent: &mut bool,
) {
    if let Ok(FlowCall {
        request,
        mut response,
        span,
    }) = call.await
    {
        // A layer may poll its inner future once and then abandon it while
        // preparing a fallback. Do not run an abandoned downstream flow.
        if response.is_closed() {
            std::future::pending::<()>().await;
        }
        req.merge_hyper(request);
        let completed = tokio::select! {
            _ = tracing::Instrument::instrument(ctrl.call_next(req, depot, res), span) => true,
            _ = response.closed() => false,
        };
        if !completed {
            std::future::pending::<()>().await;
        }
        let hyper_response = res.strip_to_hyper();
        // Keep the serialized policy/cookie headers for an error returned by
        // Tower after it consumes the response. Successful Tower responses are
        // authoritative, including any headers the layer deliberately removes.
        *res.headers_mut() = hyper_response.headers().clone();
        for name in REPRESENTATION_HEADERS {
            res.headers_mut().remove(*name);
        }
        *response_sent = true;
        let _ = response.send(hyper_response);
    }
    // Only the Tower response completes the adapter. In particular, sending
    // the inner response must not cancel Tower before it can process it.
    std::future::pending::<()>().await;
}

#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct FlowCtrlService;
impl Service<hyper::Request<ReqBody>> for FlowCtrlService {
    type Response = hyper::Response<ResBody>;
    type Error = IoError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut hyper_req: hyper::Request<ReqBody>) -> Self::Future {
        let sender = hyper_req.extensions_mut().remove::<FlowCallSender>();
        let Some(sender) = sender else {
            return ready(Err(IoError::other(
                "Salvo flow sender is missing from request extensions",
            )))
            .boxed();
        };
        let sender = sender.lock().unwrap_or_else(|e| e.into_inner()).take();
        let Some(sender) = sender else {
            return ready(Err(IoError::other(
                "Salvo flow is already running or has ended",
            )))
            .boxed();
        };
        Box::pin(async move {
            let (response, receiver) = oneshot::channel();
            sender
                .send(FlowCall {
                    request: hyper_req,
                    response,
                    span: tracing::Span::current(),
                })
                .map_err(|_| IoError::other("Salvo flow has ended"))?;
            receiver
                .await
                .map_err(|_| IoError::other("Salvo flow was cancelled"))
        })
    }
}

/// Trait for tower layer compat.
pub trait TowerLayerCompat {
    /// Converts a tower layer to a salvo handler.
    ///
    /// The layer may call its inner Salvo flow once. Downstream handlers run
    /// in the owning request task; if the layer returns early or is cancelled,
    /// any pending inner call is cancelled without taking request/depot state
    /// away from upstream middleware. Retaining or cloning a Tower request
    /// does not allow the Salvo flow to be replayed.
    fn compat<QB>(self) -> TowerLayerHandler<Self::Service, QB>
    where
        QB: TryFrom<ReqBody> + Body + Send + Sync + 'static,
        <QB as TryFrom<ReqBody>>::Error: StdError + Send + Sync + 'static,
        Self: Layer<FlowCtrlService> + Sized,
        Self::Service: tower::Service<hyper::Request<QB>> + Sync + Send + 'static,
        <Self::Service as Service<hyper::Request<QB>>>::Future: Send,
        <Self::Service as Service<hyper::Request<QB>>>::Error: StdError + Send + Sync,
    {
        TowerLayerHandler(Buffer::new(self.layer(FlowCtrlService), 32))
    }
}

impl<T> TowerLayerCompat for T where T: Layer<FlowCtrlService> + Send + Sync + Sized + 'static {}

/// Tower service compat handler.
pub struct TowerLayerHandler<Svc: Service<hyper::Request<QB>>, QB>(
    Buffer<hyper::Request<QB>, Svc::Future>,
);
impl<Svc: Service<hyper::Request<QB>>, QB> Debug for TowerLayerHandler<Svc, QB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TowerLayerHandler").finish()
    }
}

#[async_trait]
impl<Svc, QB, SB, E> Handler for TowerLayerHandler<Svc, QB>
where
    QB: TryFrom<ReqBody> + Body + Send + Sync + 'static,
    <QB as TryFrom<ReqBody>>::Error: StdError + Send + Sync + 'static,
    SB: Body + Send + Sync + 'static,
    SB::Data: Into<Bytes> + Send + fmt::Debug + 'static,
    SB::Error: StdError + Send + Sync + 'static,
    E: StdError + Send + Sync + 'static,
    Svc: Service<hyper::Request<QB>, Response = hyper::Response<SB>> + Send + 'static,
    Svc::Future: Future<Output = Result<hyper::Response<SB>, E>> + Send + 'static,
    Svc::Error: StdError + Send + Sync,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let mut svc = self.0.clone();
        if svc.ready().await.is_err() {
            tracing::error!("tower service not ready");
            res.render(StatusError::internal_server_error().cause("tower service not ready"));
            return;
        }

        // A custom body conversion can fail after stripping headers/extensions.
        // Preserve metadata even on that error path; the one-shot body itself
        // remains consumed by the attempted conversion.
        let headers = req.headers().clone();
        let extensions = req.extensions().clone();
        let stripped = req.strip_to_hyper::<QB>();
        *req.headers_mut() = headers;
        *req.extensions_mut() = extensions;
        let Ok(mut hyper_req) = stripped else {
            tracing::error!("strip request to hyper failed");
            res.render(StatusError::internal_server_error().cause("strip request to hyper failed"));
            return;
        };
        // Keep request metadata available if the Tower service never calls its
        // inner Salvo flow. The body remains owned by Tower and is not replayed.
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        hyper_req.extensions_mut().insert(sender.clone());
        let _close = CloseFlowCall(sender);
        let mut response_sent = false;
        // Salvo state stays borrowed by this adapter. Even a Tower layer that
        // retains an actively polled inner future cannot take ownership of it.
        // Completion or cancellation drops the driver and releases those borrows.
        let result = {
            tokio::select! {
                result = svc.call(hyper_req) => result,
                _ = drive_inner_call(receiver, req, depot, res, ctrl, &mut response_sent) => unreachable!(),
            }
        };
        ctrl.skip_rest();
        let mut hyper_res = match result {
            Ok(hyper_res) => hyper_res,
            Err(e) => {
                tracing::error!(error = ?e, "call tower service failed: {e}");
                res.render(
                    StatusError::internal_server_error()
                        .cause(format!("call tower service failed: {e}")),
                );
                return;
            }
        }
        .map(|res| {
            ResBody::Boxed(Box::pin(
                res.map_frame(|f| f.map_data(|data| data.into()))
                    .map_err(|e| e.into()),
            ))
        });
        // A short-circuiting layer has not serialized the prepared Salvo
        // response. Preserve its headers unless Tower explicitly replaced them;
        // Set-Cookie is additive. After a normal inner response, the returned
        // Tower headers are authoritative and the backup headers are discarded.
        for name in res.headers().keys().filter(|_| !response_sent) {
            if REPRESENTATION_HEADERS.contains(&name.as_str()) {
                continue;
            }
            if name == salvo_core::http::header::SET_COOKIE
                || !hyper_res.headers().contains_key(name)
            {
                for value in res.headers().get_all(name) {
                    hyper_res.headers_mut().append(name.clone(), value.clone());
                }
            }
        }

        res.merge_hyper(hyper_res);
    }
}

#[cfg(test)]
mod tests {
    use std::future::{Ready, ready};
    use std::io;

    use http_body_util::Full;
    use salvo_core::http::{ResBody, StatusCode};
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::{Router, handler};

    use super::*;

    fn short_circuit(
        error: bool,
        retained: Arc<std::sync::Mutex<Option<hyper::Request<ReqBody>>>>,
    ) -> impl Handler {
        tower::layer::layer_fn(move |_| {
            let retained = retained.clone();
            tower::service_fn(move |request: hyper::Request<ReqBody>| {
                *retained.lock().unwrap() = Some(request);
                ready(if error {
                    Err(io::Error::other("rejected"))
                } else {
                    Ok(hyper::Response::new(ResBody::from("short response")))
                })
            })
        })
        .compat()
    }

    #[tokio::test]
    async fn tower_short_circuit_preserves_head_and_outer_context() {
        #[handler]
        async fn probe(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            depot.insert("marker", 42_u32);
            res.add_header("x-prepared", "preserved", true).unwrap();
            res.add_header("set-cookie", "prepared=present", false)
                .unwrap();
            res.add_header("content-length", "9999", true).unwrap();
            ctrl.conn().graceful_shutdown();
            ctrl.call_next(req, depot, res).await;
            assert_eq!(req.method(), salvo_core::http::Method::HEAD);
            assert_eq!(req.uri().path(), "/original");
            assert_eq!(
                req.header::<String>("x-original").as_deref(),
                Some("present")
            );
            assert_eq!(depot.get::<u32>("marker").ok().copied(), Some(42));
            assert!(ctrl.conn().is_graceful_shutdown());
        }
        #[handler]
        async fn unreachable_goal() {
            panic!("short circuit must skip the goal");
        }
        let retained = Arc::default();
        let router = Router::with_path("original")
            .hoop(probe)
            .hoop(short_circuit(false, retained))
            .head(unreachable_goal);
        let mut response = TestClient::head("http://localhost/original")
            .add_header("x-original", "present", true)
            .send(router)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.headers().get("x-prepared").unwrap(), "preserved");
        assert_eq!(
            response.headers().get("set-cookie").unwrap(),
            "prepared=present"
        );
        assert!(response.headers().get("content-length").is_none());
        assert!(response.take_bytes(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn tower_call_error_preserves_context_for_catcher() {
        #[handler]
        async fn marker(depot: &mut Depot) {
            depot.insert("marker", 42_u32);
        }
        #[handler]
        async fn catch(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            assert_eq!(req.uri().path(), "/original");
            assert_eq!(depot.get::<u32>("marker").ok().copied(), Some(42));
            res.render("caught original request");
            ctrl.skip_rest();
        }
        let retained = Arc::default();
        let router = Router::with_path("original")
            .hoop(marker)
            .hoop(short_circuit(true, retained))
            .goal(marker);
        let service = salvo_core::Service::new(router)
            .catcher(salvo_core::catcher::Catcher::default().hoop(catch));
        let mut response = TestClient::get("http://localhost/original")
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(
            response.take_string().await.unwrap(),
            "caught original request"
        );
    }

    #[tokio::test]
    async fn tower_cancellation_recovers_context_while_request_is_retained() {
        let retained = Arc::new(std::sync::Mutex::new(None));
        let entered = Arc::new(tokio::sync::Notify::new());
        let layer = {
            let retained = retained.clone();
            let entered = entered.clone();
            tower::layer::layer_fn(move |_| {
                let retained = retained.clone();
                let entered = entered.clone();
                tower::service_fn(move |request: hyper::Request<ReqBody>| {
                    *retained.lock().unwrap() = Some(request);
                    entered.notify_one();
                    std::future::pending::<Result<hyper::Response<ResBody>, io::Error>>()
                })
            })
            .compat()
        };
        let mut req = Request::new();
        req.set_uri("/retained".parse().unwrap());
        let mut depot = Depot::new();
        depot.insert("marker", 42_u32);
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(vec![]);
        ctrl.conn().graceful_shutdown();
        {
            let handle = layer.handle(&mut req, &mut depot, &mut res, &mut ctrl);
            tokio::pin!(handle);
            tokio::select! {
                _ = &mut handle => panic!("retaining service must remain pending"),
                _ = entered.notified() => {},
            }
        }
        assert_eq!(req.uri().path(), "/retained");
        assert_eq!(depot.get::<u32>("marker").ok().copied(), Some(42));
        assert!(ctrl.conn().is_graceful_shutdown());
        assert!(retained.lock().unwrap().is_some());
        // A retained Tower request cannot execute the abandoned Salvo flow later.
        let request = retained.lock().unwrap().take().unwrap();
        assert!(FlowCtrlService.call(request).await.is_err());
    }

    #[tokio::test]
    async fn tower_response_extensions_are_not_required_for_recovery() {
        #[handler]
        async fn inner_goal(req: &mut Request, depot: &mut Depot) -> &'static str {
            assert_eq!(req.uri().path(), "/original");
            depot.insert("inner", true);
            "inner response"
        }
        #[handler]
        async fn outer(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            res.add_header("x-removed", "prepared", true).unwrap();
            res.add_header("set-cookie", "prepared=present", true)
                .unwrap();
            ctrl.call_next(req, depot, res).await;
            assert_eq!(req.uri().path(), "/original");
            assert_eq!(depot.get::<bool>("inner").ok().copied(), Some(true));
        }
        let layer = tower::layer::layer_fn(|mut inner: FlowCtrlService| {
            tower::service_fn(move |request: hyper::Request<ReqBody>| {
                let retained_extensions = request.extensions().clone();
                let response = inner.call(request);
                async move {
                    let mut response = response.await?;
                    response.extensions_mut().clear();
                    response.headers_mut().remove("x-removed");
                    drop(retained_extensions);
                    Ok::<_, io::Error>(response)
                }
            })
        });
        let router = Router::with_path("original")
            .hoop(outer)
            .hoop(layer.compat())
            .get(inner_goal);
        let mut response = TestClient::get("http://localhost/original")
            .send(router)
            .await;
        assert!(!response.headers().contains_key("x-removed"));
        assert_eq!(response.headers().get_all("set-cookie").iter().count(), 1);
        assert_eq!(response.take_string().await.unwrap(), "inner response");
    }

    #[tokio::test]
    async fn tower_error_after_inner_preserves_policy_and_cookie_headers() {
        #[handler]
        async fn prepare(res: &mut Response) {
            res.add_header("x-policy", "prepared", true).unwrap();
            res.add_header("set-cookie", "prepared=present", true)
                .unwrap();
            res.add_header("transfer-encoding", "chunked", true)
                .unwrap();
            res.add_header("trailer", "digest", true).unwrap();
        }
        #[handler]
        async fn goal() -> &'static str {
            "discarded inner response"
        }
        let layer = tower::layer::layer_fn(|mut inner: FlowCtrlService| {
            tower::service_fn(move |request: hyper::Request<ReqBody>| {
                let response = inner.call(request);
                async move {
                    drop(response.await?);
                    Err::<hyper::Response<ResBody>, _>(io::Error::other("post-processing failed"))
                }
            })
        });
        let router = Router::new().hoop(prepare).hoop(layer.compat()).get(goal);
        let response = TestClient::get("http://localhost/").send(router).await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(response.headers().get("x-policy").unwrap(), "prepared");
        assert!(!response.headers().contains_key("transfer-encoding"));
        assert!(!response.headers().contains_key("trailer"));
        assert_eq!(
            response.headers().get("set-cookie").unwrap(),
            "prepared=present"
        );
    }

    #[tokio::test]
    async fn tower_cancellation_after_inner_entry_recovers_context() {
        #[handler]
        async fn pending(req: &mut Request, depot: &mut Depot) {
            req.set_uri("/inner".parse().unwrap());
            depot.insert("inner", true);
            std::future::pending::<()>().await;
        }
        let layer = tower::layer::layer_fn(|inner: FlowCtrlService| inner).compat();
        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(vec![Arc::new(pending)]);
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                layer.handle(&mut req, &mut depot, &mut res, &mut ctrl)
            )
            .await
            .is_err()
        );
        assert_eq!(req.uri().path(), "/inner");
        assert_eq!(depot.get::<bool>("inner").ok().copied(), Some(true));
    }

    #[derive(Debug)]
    struct MarkAndWait(Arc<tokio::sync::Notify>);

    #[async_trait]
    impl Handler for MarkAndWait {
        async fn handle(
            &self,
            req: &mut Request,
            depot: &mut Depot,
            _: &mut Response,
            _: &mut FlowCtrl,
        ) {
            req.set_uri("/entered".parse().unwrap());
            depot.insert("inner", true);
            self.0.notify_one();
            std::future::pending::<()>().await;
        }
    }

    async fn retained_inner_future_preserves_context(return_early: bool) {
        let retained = Arc::new(Mutex::new(None));
        let inner_entered = Arc::new(tokio::sync::Notify::new());
        let layer_ready = Arc::new(tokio::sync::Notify::new());
        let layer = {
            let retained = retained.clone();
            let inner_entered = inner_entered.clone();
            let layer_ready = layer_ready.clone();
            tower::layer::layer_fn(move |mut inner: FlowCtrlService| {
                let retained = retained.clone();
                let inner_entered = inner_entered.clone();
                let layer_ready = layer_ready.clone();
                tower::service_fn(move |request: hyper::Request<ReqBody>| {
                    let mut future = inner.call(request);
                    let retained = retained.clone();
                    let inner_entered = inner_entered.clone();
                    let layer_ready = layer_ready.clone();
                    async move {
                        std::future::poll_fn(|cx| {
                            assert!(future.as_mut().poll(cx).is_pending());
                            Poll::Ready(())
                        })
                        .await;
                        *retained.lock().unwrap() = Some(future);
                        inner_entered.notified().await;
                        layer_ready.notify_one();
                        if return_early {
                            Ok::<_, io::Error>(hyper::Response::new(ResBody::from(
                                "early response",
                            )))
                        } else {
                            std::future::pending().await
                        }
                    }
                })
            })
            .compat()
        };
        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(vec![Arc::new(MarkAndWait(inner_entered))]);
        ctrl.conn().graceful_shutdown();
        {
            let handle = layer.handle(&mut req, &mut depot, &mut res, &mut ctrl);
            tokio::pin!(handle);
            if return_early {
                tokio::time::timeout(std::time::Duration::from_secs(2), &mut handle)
                    .await
                    .unwrap();
            } else {
                tokio::select! {
                    _ = &mut handle => panic!("layer should remain pending"),
                    _ = layer_ready.notified() => {},
                }
            }
        }
        assert_eq!(req.uri().path(), "/entered");
        assert_eq!(depot.get::<bool>("inner").ok().copied(), Some(true));
        assert!(ctrl.conn().is_graceful_shutdown());
        let future = retained.lock().unwrap().take().unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), future)
                .await
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn tower_early_response_cancels_retained_inner_future() {
        retained_inner_future_preserves_context(true).await;
    }

    #[tokio::test]
    async fn tower_cancellation_cancels_retained_inner_future() {
        retained_inner_future_preserves_context(false).await;
    }

    #[tokio::test]
    async fn tower_inner_flow_cannot_be_replayed() {
        let layer = tower::layer::layer_fn(|mut inner: FlowCtrlService| {
            tower::service_fn(move |request: hyper::Request<ReqBody>| {
                let mut replay = hyper::Request::new(ReqBody::None);
                *replay.extensions_mut() = request.extensions().clone();
                let response = inner.call(request);
                async move {
                    let response = response.await?;
                    assert!(
                        FlowCtrlService.call(replay).await.is_err(),
                        "completed flow must not be replayed"
                    );
                    Ok::<_, io::Error>(response)
                }
            })
        });
        #[handler]
        async fn goal() -> &'static str {
            "original"
        }
        let router = Router::new().hoop(layer.compat()).get(goal);
        let mut response = TestClient::get("http://localhost/").send(router).await;
        assert_eq!(response.take_string().await.unwrap(), "original");
    }

    #[tokio::test]
    async fn tower_dropped_inner_future_does_not_start_downstream() {
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[derive(Debug)]
        struct RecordCall(Arc<std::sync::atomic::AtomicBool>);
        #[async_trait]
        impl Handler for RecordCall {
            async fn handle(
                &self,
                _: &mut Request,
                _: &mut Depot,
                _: &mut Response,
                _: &mut FlowCtrl,
            ) {
                self.0.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let layer = tower::layer::layer_fn(|mut inner: FlowCtrlService| {
            tower::service_fn(move |request: hyper::Request<ReqBody>| {
                let mut future = inner.call(request);
                async move {
                    std::future::poll_fn(|cx| {
                        assert!(future.as_mut().poll(cx).is_pending());
                        Poll::Ready(())
                    })
                    .await;
                    drop(future);
                    tokio::task::yield_now().await;
                    Ok::<_, io::Error>(hyper::Response::new(ResBody::from("fallback")))
                }
            })
        });
        let router = Router::new()
            .hoop(layer.compat())
            .get(RecordCall(called.clone()));
        let mut response = TestClient::get("http://localhost/").send(router).await;
        assert_eq!(response.take_string().await.unwrap(), "fallback");
        assert!(!called.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn tower_dropped_running_inner_future_cancels_downstream() {
        #[derive(Debug)]
        struct NotifyOnDrop(Arc<tokio::sync::Notify>);
        impl Drop for NotifyOnDrop {
            fn drop(&mut self) {
                self.0.notify_one();
            }
        }
        #[derive(Debug)]
        struct WaitForCancellation {
            entered: Arc<tokio::sync::Notify>,
            cancelled: Arc<tokio::sync::Notify>,
        }
        #[async_trait]
        impl Handler for WaitForCancellation {
            async fn handle(
                &self,
                _: &mut Request,
                _: &mut Depot,
                _: &mut Response,
                _: &mut FlowCtrl,
            ) {
                let _guard = NotifyOnDrop(self.cancelled.clone());
                self.entered.notify_one();
                std::future::pending::<()>().await;
            }
        }
        let entered = Arc::new(tokio::sync::Notify::new());
        let cancelled = Arc::new(tokio::sync::Notify::new());
        let layer = {
            let entered = entered.clone();
            let cancelled = cancelled.clone();
            tower::layer::layer_fn(move |mut inner: FlowCtrlService| {
                let entered = entered.clone();
                let cancelled = cancelled.clone();
                tower::service_fn(move |request: hyper::Request<ReqBody>| {
                    let mut future = inner.call(request);
                    let entered = entered.clone();
                    let cancelled = cancelled.clone();
                    async move {
                        std::future::poll_fn(|cx| {
                            assert!(future.as_mut().poll(cx).is_pending());
                            Poll::Ready(())
                        })
                        .await;
                        entered.notified().await;
                        drop(future);
                        cancelled.notified().await;
                        Ok::<_, io::Error>(hyper::Response::new(ResBody::from("fallback")))
                    }
                })
            })
        };
        let router = Router::new()
            .hoop(layer.compat())
            .get(WaitForCancellation { entered, cancelled });
        let mut response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            TestClient::get("http://localhost/").send(router),
        )
        .await
        .unwrap();
        assert_eq!(response.take_string().await.unwrap(), "fallback");
    }

    #[derive(Debug)]
    struct RejectBody;

    impl TryFrom<ReqBody> for RejectBody {
        type Error = io::Error;
        fn try_from(_: ReqBody) -> Result<Self, Self::Error> {
            Err(io::Error::other("body conversion failed"))
        }
    }

    impl Body for RejectBody {
        type Data = Bytes;
        type Error = io::Error;
        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(None)
        }
    }

    #[tokio::test]
    async fn tower_body_conversion_error_preserves_request_metadata() {
        let layer = tower::layer::layer_fn(|_| {
            tower::service_fn(|_: hyper::Request<RejectBody>| {
                ready(Ok::<_, io::Error>(hyper::Response::new(ResBody::None)))
            })
        })
        .compat::<RejectBody>();
        let mut req = Request::new();
        req.set_uri("/original".parse().unwrap());
        req.add_header("x-original", "present", true).unwrap();
        req.extensions_mut().insert(42_u32);
        let mut res = Response::new();
        layer
            .handle(
                &mut req,
                &mut Depot::new(),
                &mut res,
                &mut FlowCtrl::new(vec![]),
            )
            .await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
        assert_eq!(req.uri().path(), "/original");
        assert_eq!(
            req.header::<String>("x-original").as_deref(),
            Some("present")
        );
        assert_eq!(req.extensions().get::<u32>(), Some(&42));
    }

    #[tokio::test]
    async fn test_tower_layer() {
        struct TestService<S> {
            inner: S,
        }

        impl<S, Req> tower::Service<Req> for TestService<S>
        where
            S: Service<Req>,
        {
            type Response = S::Response;
            type Error = S::Error;
            type Future = S::Future;

            fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                self.inner.poll_ready(cx)
            }

            fn call(&mut self, req: Req) -> Self::Future {
                self.inner.call(req)
            }
        }

        struct MyServiceLayer;

        impl<S> Layer<S> for MyServiceLayer {
            type Service = TestService<S>;

            fn layer(&self, inner: S) -> Self::Service {
                TestService { inner }
            }
        }

        #[handler]
        async fn hello() -> &'static str {
            "Hello World"
        }
        let router = Router::new().hoop(MyServiceLayer.compat()).get(hello);
        assert_eq!(
            TestClient::get("http://127.0.0.1:8698")
                .send(router)
                .await
                .take_string()
                .await
                .unwrap(),
            "Hello World"
        );
    }

    #[derive(Clone)]
    struct NotReadyService;

    impl Service<hyper::Request<ReqBody>> for NotReadyService {
        type Response = hyper::Response<Full<Bytes>>;
        type Error = io::Error;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Err(io::Error::other("not ready")))
        }

        fn call(&mut self, _req: hyper::Request<ReqBody>) -> Self::Future {
            ready(Ok(hyper::Response::new(Full::new(Bytes::from_static(
                b"unused",
            )))))
        }
    }

    #[derive(Clone)]
    struct FailingService;

    impl Service<hyper::Request<ReqBody>> for FailingService {
        type Response = hyper::Response<Full<Bytes>>;
        type Error = io::Error;
        type Future = Ready<Result<Self::Response, io::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: hyper::Request<ReqBody>) -> Self::Future {
            ready(Err(io::Error::other("boom")))
        }
    }

    #[tokio::test]
    async fn test_tower_service_compat_ready_error_renders_consistent_message() {
        let mut response = TestClient::get("http://127.0.0.1:8698")
            .send(NotReadyService.compat())
            .await;

        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        let ResBody::Error(error) = response.take_body() else {
            panic!("expected an error response body");
        };
        assert_eq!(error.code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            error.cause.as_ref().map(ToString::to_string).as_deref(),
            Some("tower service not ready")
        );
    }

    #[tokio::test]
    async fn test_tower_service_compat_call_error_renders_consistent_message() {
        let mut response = TestClient::get("http://127.0.0.1:8698")
            .send(FailingService.compat())
            .await;

        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        let ResBody::Error(error) = response.take_body() else {
            panic!("expected an error response body");
        };
        assert_eq!(error.code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            error.cause.as_ref().map(ToString::to_string).as_deref(),
            Some("call tower service failed: boom")
        );
    }
}
