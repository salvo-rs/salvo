//! Adapters for [`tower::Layer`](https://docs.rs/tower/latest/tower/trait.Layer.html) and
//! [`tower::Service`](https://docs.rs/tower/latest/tower/trait.Service.html).
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
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     let router = Router::new().hoop(limit).get(hello);
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
use std::error::Error as StdError;
use std::fmt;
use std::io::{Error as IoError, ErrorKind};
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::{BoxFuture, FutureExt};
use http_body_util::BodyExt;
use hyper::body::{Body, Bytes};
use tower::buffer::Buffer;
use tower::{Layer, Service, ServiceExt};

use salvo_core::http::{ReqBody, ResBody, StatusError};
use salvo_core::{async_trait, hyper, Depot, FlowCtrl, Handler, Request, Response};

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
    T: Service<hyper::Request<ReqBody>, Response = hyper::Response<SB>, Future = Fut> + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<hyper::Response<SB>, E>> + Send + 'static,
{
}

/// Tower service compat handler.
pub struct TowerServiceHandler<Svc, QB>(Svc, PhantomData<QB>);

#[async_trait]
impl<Svc, QB, SB, E, Fut> Handler for TowerServiceHandler<Svc, QB>
where
    QB: TryFrom<ReqBody> + Body + Send + Sync + 'static,
    <QB as TryFrom<ReqBody>>::Error: StdError + Send + Sync + 'static,
    SB: Body + Send + Sync + 'static,
    SB::Data: Into<Bytes> + Send + fmt::Debug + 'static,
    SB::Error: StdError + Send + Sync + 'static,
    E: StdError + Send + Sync + 'static,
    Svc: Service<hyper::Request<QB>, Response = hyper::Response<SB>, Future = Fut> + Send + Sync + Clone + 'static,
    Svc::Error: StdError + Send + Sync + 'static,
    Fut: Future<Output = Result<hyper::Response<SB>, E>> + Send + 'static,
{
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let mut svc = self.0.clone();
        if svc.ready().await.is_err() {
            tracing::error!("tower service not ready.");
            res.render(StatusError::internal_server_error().cause("tower service not ready."));
            return;
        }
        let hyper_req = match req.strip_to_hyper::<QB>() {
            Ok(hyper_req) => hyper_req,
            Err(_) => {
                tracing::error!("strip request to hyper failed.");
                res.render(StatusError::internal_server_error().cause("strip request to hyper failed."));
                return;
            }
        };

        let hyper_res = match svc.call(hyper_req).await {
            Ok(hyper_res) => hyper_res,
            Err(e) => {
                tracing::error!(error = ?e, "call tower service failed: {}", e);
                res.render(StatusError::internal_server_error().cause(format!("call tower service failed: {}", e)));
                return;
            }
        }
        .map(|res| ResBody::Boxed(Box::pin(res.map_frame(|f| f.map_data(|data| data.into())).map_err(|e|e.into()))));

        res.merge_hyper(hyper_res);
    }
}

struct FlowCtrlInContext {
    ctrl: FlowCtrl,
    request: Request,
    depot: Depot,
    response: Response,
}
impl FlowCtrlInContext {
    fn new(ctrl: FlowCtrl, request: Request, depot: Depot, response: Response) -> Self {
        Self {
            ctrl,
            request,
            depot,
            response,
        }
    }
}
struct FlowCtrlOutContext {
    ctrl: FlowCtrl,
    request: Request,
    depot: Depot,
}
impl FlowCtrlOutContext {
    fn new(ctrl: FlowCtrl, request: Request, depot: Depot) -> Self {
        Self { ctrl, request, depot }
    }
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
        let ctx = hyper_req.extensions_mut().remove::<Arc<FlowCtrlInContext>>().and_then(Arc::into_inner);
        let Some(FlowCtrlInContext {
            mut ctrl,
            mut request,
            mut depot,
            mut response,
        }) = ctx
        else {
            return futures_util::future::ready(Err(IoError::new(
                ErrorKind::Other,
                "`FlowCtrlInContext` should exists in request extension.".to_owned(),
            )))
            .boxed();
        };
        request.merge_hyper(hyper_req);
        Box::pin(async move {
            ctrl.call_next(&mut request, &mut depot, &mut response).await;
            response
                .extensions
                .insert(Arc::new(FlowCtrlOutContext::new(ctrl, request, depot)));
            Ok(response.strip_to_hyper())
        })
    }
}

/// Trait for tower layer compat.
pub trait TowerLayerCompat {
    /// Converts a tower layer to a salvo handler.
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
pub struct TowerLayerHandler<Svc: Service<hyper::Request<QB>>, QB>(Buffer<hyper::Request<QB>, Svc::Future>);

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
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let mut svc = self.0.clone();
        if svc.ready().await.is_err() {
            tracing::error!("tower service not ready.");
            res.render(StatusError::internal_server_error().cause("tower service not ready."));
            return;
        }

        let mut hyper_req = match req.strip_to_hyper::<QB>() {
            Ok(hyper_req) => hyper_req,
            Err(_) => {
                tracing::error!("strip request to hyper failed.");
                res.render(StatusError::internal_server_error().cause("strip request to hyper failed."));
                return;
            }
        };
        let ctx = FlowCtrlInContext::new(
            std::mem::take(ctrl),
            std::mem::take(req),
            std::mem::take(depot),
            std::mem::take(res),
        );
        hyper_req.extensions_mut().insert(Arc::new(ctx));

        let mut hyper_res = match svc.call(hyper_req).await {
            Ok(hyper_res) => hyper_res,
            Err(e) => {
                tracing::error!(error = ?e, "call tower service failed: {}", e);
                res.render(StatusError::internal_server_error().cause(format!("call tower service failed: {}", e)));
                return;
            }
        }
        .map(|res| ResBody::Boxed(Box::pin(res.map_frame(|f| f.map_data(|data| data.into())).map_err(|e|e.into()))));
        let origin_depot = depot;
        let origin_ctrl = ctrl;

        let ctx = hyper_res.extensions_mut().remove::<Arc<FlowCtrlOutContext>>().and_then(Arc::into_inner);
        if let Some(FlowCtrlOutContext { ctrl, request, depot }) = ctx
        {
            *origin_depot = depot;
            *origin_ctrl = ctrl;
            *req = request;
        } else {
            tracing::debug!(
                "`FlowCtrlOutContext` does not exists in response extensions, `FlowCtrlService` may not be used."
            );
        }

        res.merge_hyper(hyper_res);
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::{handler, Router};

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
            TestClient::get("http://127.0.0.1:5800")
                .send(router)
                .await
                .take_string()
                .await
                .unwrap(),
            "Hello World"
        );
    }
}
