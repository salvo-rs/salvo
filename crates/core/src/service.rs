use std::future::Future;
use std::io::Error as IoError;
use std::pin::Pin;
use std::sync::Arc;

use futures_util::future;

use crate::addr::SocketAddr;
use crate::catcher::CatcherImpl;
use crate::http::header::CONTENT_TYPE;
use crate::http::{Mime, Request, Response, StatusCode};
use crate::routing::{FlowCtrl, PathState, Router};
use crate::transport::Transport;
use crate::{Catcher, Depot};

/// Service http request.
pub struct Service {
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}

impl Service {
    /// Create a new Service with a [`Router`].
    #[inline]
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

    /// Get router in this `Service`.
    #[inline]
    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    /// When the response code is 400-600 and the body is empty, capture and set the return value.
    /// If catchers is not set, the default [`CatcherImpl`] will be used.
    ///
    /// # Example
    ///
    /// ```
    /// # use salvo_core::prelude::*;
    /// # use salvo_core::Catcher;
    ///
    /// struct Handle404;
    /// impl Catcher for Handle404 {
    ///     fn catch(&self, _req: &Request, _depot: &Depot, res: &mut Response) -> bool {
    ///         if let Some(StatusCode::NOT_FOUND) = res.status_code() {
    ///             res.render("Custom 404 Error Page");
    ///             true
    ///         } else {
    ///             false
    ///         }
    ///     }
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let catchers: Vec<Box<dyn Catcher>> = vec![Box::new(Handle404)];
    ///     Service::new(Router::new()).with_catchers(catchers);
    /// }
    /// ```
    #[inline]
    pub fn with_catchers<T>(mut self, catchers: T) -> Self
    where
        T: Into<Arc<Vec<Box<dyn Catcher>>>>,
    {
        self.catchers = catchers.into();
        self
    }

    /// Get catchers list.
    #[inline]
    pub fn catchers(&self) -> Arc<Vec<Box<dyn Catcher>>> {
        self.catchers.clone()
    }

    /// Sets allowed media types list and returns `Self` for write code chained.
    ///
    /// # Example
    ///
    /// ```
    /// # use salvo_core::prelude::*;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let service = Service::new(Router::new()).with_allowed_media_types(vec![mime::TEXT_PLAIN]);
    /// # }
    /// ```
    #[inline]
    pub fn with_allowed_media_types<T>(mut self, allowed_media_types: T) -> Self
    where
        T: Into<Arc<Vec<Mime>>>,
    {
        self.allowed_media_types = allowed_media_types.into();
        self
    }

    /// Get allowed media types list.
    #[inline]
    pub fn allowed_media_types(&self) -> Arc<Vec<Mime>> {
        self.allowed_media_types.clone()
    }

    #[doc(hidden)]
    #[inline]
    pub fn hyper_handler(&self, remote_addr: Option<SocketAddr>) -> HyperHandler {
        HyperHandler {
            remote_addr,
            router: self.router.clone(),
            catchers: self.catchers.clone(),
            allowed_media_types: self.allowed_media_types.clone(),
        }
    }

    /// Handle [`Request`] and returns [`Response`].
    ///
    /// This function is useful for testing application.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::prelude::*;
    /// use salvo_core::test::{ResponseExt, TestClient};
    ///
    /// #[handler]
    /// async fn hello_world() -> &'static str {
    ///     "Hello World"
    /// }
    /// #[tokio::main]
    /// async fn main() {
    ///     let service: Service = Router::new().get(hello_world).into();
    ///     let mut res = TestClient::get("http://127.0.0.1:7878").send(&service).await;
    ///     assert_eq!(res.take_string().await.unwrap(), "Hello World");
    /// }
    /// ```
    #[inline]
    pub async fn handle(&self, request: impl Into<Request>) -> Response {
        self.hyper_handler(None).handle(request.into()).await
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

    #[inline]
    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    #[inline]
    fn call(&mut self, target: &T) -> Self::Future {
        future::ok(self.hyper_handler(target.remote_addr()))
    }
}

impl<T> From<T> for Service
where
    T: Into<Arc<Router>>,
{
    #[inline]
    fn from(router: T) -> Self {
        Service::new(router)
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub struct HyperHandler {
    pub(crate) remote_addr: Option<SocketAddr>,
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}
impl HyperHandler {
    /// Handle [`Request`] and returns [`Response`].
    #[inline]
    pub fn handle(&self, mut req: Request) -> impl Future<Output = Response> {
        let catchers = self.catchers.clone();
        let allowed_media_types = self.allowed_media_types.clone();
        req.remote_addr = self.remote_addr.clone();
        let mut res = Response::new();
        let mut depot = Depot::new();
        let mut path_state = PathState::new(req.uri().path());
        let router = self.router.clone();

        async move {
            if let Some(dm) = router.detect(&mut req, &mut path_state) {
                req.params = path_state.params;
                let mut ctrl = FlowCtrl::new([&dm.hoops[..], &[dm.handler]].concat());
                ctrl.call_next(&mut req, &mut depot, &mut res).await;
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
            } else if res.body.is_none() && !has_error {
                // check for avoid warning when errors (404 etc.)
                tracing::warn!(
                    uri = ?req.uri(),
                    method = req.method().as_str(),
                    "Http response content type header not set"
                );
            }
            if res.body.is_none() && has_error {
                let mut catched = false;
                for catcher in catchers.iter() {
                    if catcher.catch(&req, &depot, &mut res) {
                        catched = true;
                        break;
                    }
                }
                if !catched {
                    CatcherImpl.catch(&req, &depot, &mut res);
                }
            }
            #[cfg(debug_assertions)]
            if let hyper::Method::HEAD = *req.method() {
                if !res.body.is_none() {
                    tracing::warn!("request with head method should not have body: https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods/HEAD");
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

    #[inline]
    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    #[inline]
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
    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_service() {
        #[handler(internal)]
        async fn before1(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            res.render(Text::Plain("before1"));
            if req.query::<String>("b").unwrap_or_default() == "1" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[handler(internal)]
        async fn before2(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            res.render(Text::Plain("before2"));
            if req.query::<String>("b").unwrap_or_default() == "2" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[handler(internal)]
        async fn before3(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            res.render(Text::Plain("before3"));
            if req.query::<String>("b").unwrap_or_default() == "3" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[handler(internal)]
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
            TestClient::get(format!("http://127.0.0.1:7979/level1/level2/hello?b={}", b))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
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
