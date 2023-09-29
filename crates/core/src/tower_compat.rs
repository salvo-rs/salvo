//! Tower service compat.
use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::io::{Error as IoError, ErrorKind};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use http_body_util::BodyExt;
use hyper::body::{Body, Bytes, Frame};
use tower::{Layer, Service, ServiceExt};

use crate::http::{ReqBody, ResBody, StatusError};
use crate::{async_trait, Depot, FlowCtrl, Handler, Request, Response};

/// Trait for tower service compat.
pub trait TowerServiceCompat<B, E, Fut> {
    /// Converts a tower service to a salvo handler.
    fn compat(self) -> TowerServiceHandler<Self>
    where
        Self: Sized,
    {
        TowerServiceHandler(self)
    }
}

impl<T, B, E, Fut> TowerServiceCompat<B, E, Fut> for T
where
    B: Body + Send + Sync + 'static,
    B::Data: Into<Bytes> + Send + fmt::Debug + 'static,
    B::Error: StdError + Send + Sync + 'static,
    E: StdError + Send + Sync + 'static,
    T: Service<hyper::Request<ReqBody>, Response = hyper::Response<B>, Future = Fut> + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<hyper::Response<B>, E>> + Send + 'static,
{
}

/// Tower service compat handler.
pub struct TowerServiceHandler<Svc>(Svc);

#[async_trait]
impl<Svc, B, E, Fut> Handler for TowerServiceHandler<Svc>
where
    B: Body + Send + Sync + 'static,
    B::Data: Into<Bytes> + Send + fmt::Debug + 'static,
    B::Error: StdError + Send + Sync + 'static,
    E: StdError + Send + Sync + 'static,
    Svc: Service<hyper::Request<ReqBody>, Response = hyper::Response<B>, Future = Fut> + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<hyper::Response<B>, E>> + Send + 'static,
{
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let mut svc = self.0.clone();
        if let Err(_) = svc.ready().await {
            tracing::error!("tower service not ready.");
            res.render(StatusError::internal_server_error().cause("tower service not ready."));
            return;
        }
        let hyper_req = match req.strip_to_hyper() {
            Ok(hyper_req) => hyper_req,
            Err(_) => {
                tracing::error!("strip request to hyper failed.");
                res.render(StatusError::internal_server_error().cause("strip request to hyper failed."));
                return;
            }
        };

        let hyper_res = match svc.call(hyper_req).await {
            Ok(hyper_res) => hyper_res,
            Err(_) => {
                tracing::error!("call tower service failed.");
                res.render(StatusError::internal_server_error().cause("call tower service failed."));
                return;
            }
        }
        .map(|res| {
            ResBody::Boxed(Box::pin(
                res.map_frame(|f| match f.into_data() {
                    //TODO: should use Frame::map_data after new version of hyper is released.
                    Ok(data) => Frame::data(data.into()),
                    Err(frame) => Frame::trailers(frame.into_trailers().expect("frame must be trailers")),
                })
                .map_err(|e| e.into()),
            ))
        });

        res.merge_hyper(hyper_res);
    }
}

struct ResponseService(Option<hyper::Response<ResBody>>);
impl<Req> Service<Req> for ResponseService {
    type Response = hyper::Response<ResBody>;
    type Error = IoError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Req) -> Self::Future {
        let res = self.0.take();
        Box::pin(async move {
            if let Some(res) = res {
                Ok(res)
            } else {
                Err(IoError::new(
                    ErrorKind::Other,
                    "response is none or called in response service.".to_owned(),
                ))
            }
        })
    }
}

/// Trait for tower layer compat.
pub trait TowerLayerCompat<S> {
    /// Converts a tower layer to a salvo handler.
    fn compat(self) -> TowerLayerHandler<Self, S>
    where
        Self: Sized,
    {
        TowerLayerHandler(self, PhantomData)
    }
}

impl<T, S> TowerLayerCompat<S> for T where T: Layer<S> + Send + Sync + Sized + 'static {}

/// Tower service compat handler.
pub struct TowerLayerHandler<L, S>(L, PhantomData<S>);

#[async_trait]
impl<L, S, B> Handler for TowerLayerHandler<L, S>
where
    B: Body + Send + Sync + 'static,
    B::Data: Into<Bytes> + Sync + Send + fmt::Debug + 'static,
    B::Error: StdError + Send + Sync + 'static,
    S: Sync + Send + 'static,
    L: Layer<ResponseService> + Sync + Send + 'static,
    L::Service: Service<hyper::Request<ReqBody>, Response = hyper::Response<B>> + Sync + Send + 'static,
    <L::Service as Service<hyper::Request<ReqBody>>>::Future: Sync + Send,
    <L::Service as Service<hyper::Request<ReqBody>>>::Error: StdError + Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        // ctrl.call_next(req, depot, res).await;

        // let hyper_res = res.strip_to_hyper();
        // let mut svc = self.0.layer(ResponseService(Some(hyper_res)));
        // if let Err(_) = svc.ready().await {
        //     tracing::error!("tower service not ready.");
        //     res.render(StatusError::internal_server_error().cause("tower service not ready."));
        //     return;
        // }
        // let hyper_req = match req.strip_to_hyper() {
        //     Ok(hyper_req) => hyper_req,
        //     Err(_) => {
        //         tracing::error!("strip request to hyper failed.");
        //         res.render(StatusError::internal_server_error().cause("strip request to hyper failed."));
        //         return;
        //     }
        // };

        // let hyper_res = match svc.call(hyper_req).await {
        //     Ok(hyper_res) => hyper_res,
        //     Err(_) => {
        //         tracing::error!("call tower service failed.");
        //         res.render(StatusError::internal_server_error().cause("call tower service failed."));
        //         return;
        //     }
        // }
        // .map(|res| {
        //     ResBody::Boxed(Box::pin(
        //         res.map_frame(|f| match f.into_data() {
        //             //TODO: should use Frame::map_data after new version of hyper is released.
        //             Ok(data) => Frame::data(data.into()),
        //             Err(frame) => Frame::trailers(frame.into_trailers().expect("frame must be trailers")),
        //         })
        //         .map_err(|e| e.into()),
        //     ))
        // });

        // res.merge_hyper(hyper_res);
    }
}
