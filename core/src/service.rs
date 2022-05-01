use std::future::Future;
use std::io::Error as IoError;
use std::pin::Pin;
use std::sync::Arc;

use futures_util::future;
use once_cell::sync::Lazy;

use crate::addr::SocketAddr;
use crate::catcher::CatcherImpl;
use crate::http::header::CONTENT_TYPE;
use crate::http::{Mime, Request, Response, StatusCode};
use crate::routing::{FlowCtrl, PathState, Router};
use crate::transport::Transport;
use crate::{Catcher, Depot};

static DEFAULT_CATCHERS: Lazy<Vec<Box<dyn Catcher>>> = Lazy::new(|| vec![Box::new(CatcherImpl::new())]);
/// Service http request.
pub struct Service {
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}

impl Service {
    /// Create a new Service with a router.
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

    /// Get root router.
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

    /// Get catchers list.
    pub fn catchers(&self) -> Arc<Vec<Box<dyn Catcher>>> {
        self.catchers.clone()
    }

    /// Set allowed media types list and returns Self for wite code chained.
    pub fn with_allowed_media_types<T>(mut self, allowed_media_types: T) -> Self
    where
        T: Into<Arc<Vec<Mime>>>,
    {
        self.allowed_media_types = allowed_media_types.into();
        self
    }

    /// Get allowed media types list.
    pub fn allowed_media_types(&self) -> Arc<Vec<Mime>> {
        self.allowed_media_types.clone()
    }

    /// Handle ```Request``` and returns ```Response```.
    pub async fn handle(&self, request: Request) -> Response {
        let handler = HyperHandler {
            remote_addr: None,
            router: self.router.clone(),
            catchers: self.catchers.clone(),
            allowed_media_types: self.allowed_media_types.clone(),
        };
        handler.handle(request).await
    }
}
impl<'t, T> hyper::service::Service<&'t T> for Service
where
    T: Transport,
{
    type Response = HyperHandler;
    type Error = IoError;

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

impl From<Router> for Service {
    fn from(router: Router) -> Self {
        Service::new(router)
    }
}

#[doc(hidden)]
pub struct HyperHandler {
    pub(crate) remote_addr: Option<SocketAddr>,
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}
impl HyperHandler {
    pub fn handle(&self, mut req: Request) -> impl Future<Output = Response> {
        let catchers = self.catchers.clone();
        let allowed_media_types = self.allowed_media_types.clone();
        req.remote_addr = self.remote_addr.clone();
        let mut res = Response::new();
        let mut depot = Depot::new();
        let mut path_state = PathState::new(req.uri().path());
        res.cookies = req.cookies().clone();
        let router = self.router.clone();

        async move {
            if let Some(dm) = router.detect(&mut req, &mut path_state) {
                req.params = path_state.params;
                let mut ctrl = FlowCtrl::new([&dm.hoops[..], &[dm.handler]].concat());
                ctrl.call_next(&mut req, &mut depot, &mut res).await;
                if ctrl.has_next() {
                    let allow_reset = res
                        .status_code()
                        .map(|code| !code.is_success() || code.is_redirection())
                        .unwrap_or(false);
                    if !allow_reset {
                        tracing::error!(url = ?req.uri(), "FlowCtrl has_next should return false");
                    }
                }
            } else {
                res.set_status_code(StatusCode::NOT_FOUND);
            }

            if res.status_code().is_none() {
                if res.body.is_none() {
                    res.set_status_code(StatusCode::NOT_FOUND);
                } else {
                    res.set_status_code(StatusCode::OK);
                }
            }

            let status = res.status_code().unwrap();
            let has_error = status.is_client_error() || status.is_server_error();
            if let Some(value) = res.headers().get(CONTENT_TYPE) {
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
                    res.set_status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                }
            } else if res.body.is_none() {
                //avoid warning when errors (404 etc.)
                tracing::warn!(
                    uri = ?req.uri(),
                    method = req.method().as_str(),
                    "Http response content type header is not set"
                );
            }
            if res.body.is_none() && has_error {
                for catcher in catchers.iter().chain(DEFAULT_CATCHERS.iter()) {
                    if catcher.catch(&req, &depot, &mut res) {
                        break;
                    }
                }
            }
            #[cfg(debug_assertions)]
            if let hyper::Method::HEAD = *req.method() {
                if let Some(body) = res.body() {
                    if !body.is_empty() {
                        tracing::warn!("request with head method should have empty body: https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/HEAD");
                    }
                }
            }
            res
        }
    }
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
        let response = self.handle(req.into());
        let fut = async move {
            let mut hyper_response = hyper::Response::<hyper::Body>::new(hyper::Body::empty());
            response.await.write_back(&mut hyper_response).await;
            Ok(hyper_response)
        };
        Box::pin(fut)
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    #[tokio::test]
    async fn test_service() {
        #[fn_handler]
        async fn before1(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            res.render(Text::Plain("before1"));
            if req.get_query::<String>("b").unwrap_or_default() == "1" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[fn_handler]
        async fn before2(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            res.render(Text::Plain("before2"));
            if req.get_query::<String>("b").unwrap_or_default() == "2" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[fn_handler]
        async fn before3(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            res.render(Text::Plain("before3"));
            if req.get_query::<String>("b").unwrap_or_default() == "3" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[fn_handler]
        async fn hello() -> Result<&'static str, ()> {
            Ok("hello")
        }
        let router = Router::with_path("level1").hoop(before1).push(
            Router::with_hoop(before2)
                .path("level2")
                .push(Router::with_hoop(before3).path("hello").handle(hello)),
        );
        let service = Service::new(router);

        async fn access(service: &Service, b: &str) -> String {
            let req: Request = hyper::Request::builder()
                .method("GET")
                .uri(format!("http://127.0.0.1:7979/level1/level2/hello?b={}", b))
                .body(hyper::Body::empty())
                .unwrap()
                .into();
            service.handle(req).await.take_text().await.unwrap()
        }
        let content = access(&service, "").await;
        assert_eq!(content, "before1before2before3hello");
        let content = access(&service, "1").await;
        assert_eq!(content, "before1");
        let content = access(&service, "2").await;
        assert_eq!(content, "before1before2");
        let content = access(&service, "3").await;
        assert_eq!(content, "before1before2before3");
    }
}
