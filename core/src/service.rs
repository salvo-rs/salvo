use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use futures::future;
use once_cell::sync::Lazy;

use crate::catcher;
use crate::http::header::CONTENT_TYPE;
use crate::http::{Mime, Request, Response, StatusCode};
use crate::routing::{PathState, Router};
use crate::transport::Transport;
use crate::{Catcher, Depot};

static DEFAULT_CATCHERS: Lazy<Vec<Box<dyn Catcher>>> = Lazy::new(catcher::defaults::get);
pub struct Service {
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}

impl Service {
    pub fn new<T>(router: T) -> Service
    where
        T: Into<Arc<Router>>,
    {
        Service {
            router: router.into(),
            catchers: Arc::new(vec![]),
            allowed_media_types: Arc::new(vec![]),
        }
    }
    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }
    /// when the response code is 400-600 and the body is empty, capture and set the return value.
    /// By default, it is the built-in default html page.
    pub fn with_catchers<T>(mut self, catchers: T) -> Self
    where
        T: Into<Arc<Vec<Box<dyn Catcher>>>>,
    {
        self.catchers = catchers.into();
        self
    }
    pub fn catchers(&self) -> Arc<Vec<Box<dyn Catcher>>> {
        self.catchers.clone()
    }
    pub fn with_allowed_media_types<T>(mut self, allowed_media_types: T) -> Self
    where
        T: Into<Arc<Vec<Mime>>>,
    {
        self.allowed_media_types = allowed_media_types.into();
        self
    }
    pub fn allowed_media_types(&self) -> Arc<Vec<Mime>> {
        self.allowed_media_types.clone()
    }
}
impl<'t, T> hyper::service::Service<&'t T> for Service
where
    T: Transport,
{
    type Response = HyperHandler;
    type Error = std::io::Error;

    // type Future = Pin<Box<(dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static)>>;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, target: &T) -> Self::Future {
        let remote_addr = target.remote_addr();
        future::ok(HyperHandler {
            remote_addr,
            router: self.router.clone(),
            catchers: self.catchers.clone(),
            allowed_media_types: self.allowed_media_types.clone(),
        })
    }
}

pub struct HyperHandler {
    pub(crate) remote_addr: Option<SocketAddr>,
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}
#[allow(clippy::type_complexity)]
impl hyper::service::Service<hyper::Request<hyper::body::Body>> for HyperHandler {
    type Response = hyper::Response<hyper::body::Body>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn call(&mut self, req: hyper::Request<hyper::body::Body>) -> Self::Future {
        let catchers = self.catchers.clone();
        let allowed_media_types = self.allowed_media_types.clone();
        let mut request = Request::from_hyper(req);
        request.set_remote_addr(self.remote_addr);
        let mut response = Response::new();
        let mut depot = Depot::new();
        let mut path_state = PathState::new(request.uri().path());
        response.cookies = request.cookies().clone();

        let router = self.router.clone();
        let fut = async move {
            if let Some(dm) = router.detect(&mut request, &mut path_state) {
                request.params = path_state.params;
                for handler in [&dm.befores[..], &[dm.handler]].concat() {
                    handler.handle(&mut request, &mut depot, &mut response).await;
                    if response.is_commited() {
                        break;
                    }
                }
                // Ensure these after handlers must be executed
                for handler in &dm.afters {
                    handler.handle(&mut request, &mut depot, &mut response).await;
                }
                response.commit();
            } else {
                response.set_status_code(StatusCode::NOT_FOUND);
            }

            let mut hyper_response = hyper::Response::<hyper::Body>::new(hyper::Body::empty());

            if response.status_code().is_none() {
                if response.body.is_none() {
                    response.set_status_code(StatusCode::NOT_FOUND);
                } else {
                    response.set_status_code(StatusCode::OK);
                }
            }

            let status = response.status_code().unwrap();
            let has_error = status.is_client_error() || status.is_server_error();
            if let Some(value) = response.headers().get(CONTENT_TYPE) {
                let mut is_allowed = false;
                if let Ok(value) = value.to_str() {
                    if allowed_media_types.is_empty() {
                        is_allowed = true;
                    } else {
                        let ctype: Result<Mime, _> = value.parse();
                        if let Ok(ctype) = ctype {
                            for mime in &*allowed_media_types {
                                if mime.type_() == ctype.type_() && mime.subtype() == ctype.subtype() {
                                    is_allowed = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                if !is_allowed {
                    response.set_status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                }
            } else {
                tracing::warn!(
                    uri = ?request.uri(),
                    method = request.method().as_str(),
                    "Http response content type header is not set"
                );
            }
            if response.body.is_none() && has_error {
                for catcher in catchers.iter().chain(DEFAULT_CATCHERS.iter()) {
                    if catcher.catch(&request, &mut response) {
                        break;
                    }
                }
            }
            response.write_back(&mut request, &mut hyper_response).await;
            Ok(hyper_response)
        };
        Box::pin(fut)
    }
}
