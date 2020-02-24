use std::sync::Arc;
use std::time::Duration;
use std::pin::Pin;

use hyper::Server as HyperServer;

use crate::{Protocol, Catcher, Depot};
use crate::http::{StatusCode, Request, Response, Mime};
use crate::http::header::CONTENT_TYPE;
use crate::routing::Router;
use crate::catcher;
use crate::logging;
use super::pick_port;

use std::net::{SocketAddr, ToSocketAddrs};
use futures::{future, Future};
/// A settings struct containing a set of timeouts which can be applied to a server.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Timeouts {
    /// Controls the timeout for keep alive connections.
    ///
    /// The default is `Some(Duration::from_secs(5))`.
    ///
    /// NOTE: Setting this to None will have the effect of turning off keep alive.
    pub keep_alive: Option<Duration>,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            keep_alive: Some(Duration::from_secs(5)),
        }
    }
}

/// The main `Novel` type: used to mount routes and catchers and launch the
/// application.
pub struct Server {
    pub router: Arc<Router>,
    pub config: Arc<ServerConfig>
}
pub struct ServerConfig{
    pub timeouts: Timeouts,

    /// Protocol of the incoming requests
    ///
    /// This is automatically set by the `http` and `https` functions, but
    /// can be set if you are manually constructing the hyper `http` instance.
    pub protocol: Protocol,

    /// Default host address to use when none is provided
    ///
    /// When set, this provides a default host for any requests that don't
    /// provide one.  When unset, any request without a host specified
    /// will fail.
    pub local_addr: Option<SocketAddr>,

    pub catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub allowed_media_types: Arc<Vec<Mime>>,
}
impl ServerConfig {
    pub fn new()->ServerConfig{
        // let mimes = vec![
        //     mime::APPLICATION_JSON,
        //     mime::APPLICATION_JAVASCRIPT,
        //     mime::APPLICATION_OCTET_STREAM,
        //     mime::APPLICATION_MSGPACK,
        //     mime::APPLICATION_OCTET_STREAM,
        //     mime::APPLICATION_PDF,
        //     mime::TEXT_PLAIN,
        //     mime::TEXT_HTML,
        //     mime::TEXT_CSS,
        //     mime::TEXT_JAVASCRIPT,
        //     mime::TEXT_XML,
        //     mime::TEXT_EVENT_STREAM,
        //     mime::TEXT_CSV,
        //     mime::TEXT_VCARD,
        //     mime::IMAGE_JPEG,
        //     mime::IMAGE_GIF,
        //     mime::IMAGE_PNG,
        //     mime::IMAGE_BMP,
        //     mime::IMAGE_SVG,
        //     mime::FONT_WOFF,
        //     mime::FONT_WOFF2,
        // ];
        ServerConfig{
            protocol: Protocol::http(),
            local_addr: None,
            timeouts: Timeouts::default(),
            catchers: Arc::new(catcher::defaults::get()),
            allowed_media_types: Arc::new(vec![]),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig::new()
    }
}

impl Server {
    pub fn new(router: Router) -> Server{
        let config = ServerConfig::default();
        Server{
            router: Arc::new(router),
            config: Arc::new(config),
        }
    }

    pub fn with_config(router: Router, config: ServerConfig) -> Server {
        Server{
            router: Arc::new(router),
            config: Arc::new(config),
        }
    }

    pub fn with_addr<T>(router: Router, addr: T) -> Server where T: ToSocketAddrs {
        let mut config = ServerConfig::default();
        config.local_addr = addr.to_socket_addrs().unwrap().next();
        Server{
            router: Arc::new(router),
            config: Arc::new(config),
        }
    }

    pub fn serve(self) -> impl Future<Output=Result<(), hyper::error::Error>> + Send + 'static {
        let addr: SocketAddr = self.config.local_addr.unwrap_or_else(|| {
            let port = pick_port::pick_unused_port().expect("Pick unused port failed");
            let addr = format!("localhost:{}", port).to_socket_addrs().unwrap().next().unwrap();
            warn!(logging::logger(), "Local address is not set, randrom address used.");
            addr
        });
        info!(logging::logger(), "Server will be served"; "address" => addr);
        HyperServer::bind(&addr)
            .tcp_keepalive(self.config.timeouts.keep_alive)
            .serve(self)
    }
}
impl<T>  hyper::service::Service<T> for Server {
    type Response = HyperHandler;
    type Error = std::io::Error;
    // type Future = Pin<Box<(dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static)>>;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, _: T) -> Self::Future {
        future::ok(HyperHandler{
            router: self.router.clone(),
            config: self.config.clone(),
        })
    }
}
pub struct HyperHandler {
    router: Arc<Router>,
    config: Arc<ServerConfig>,
}
impl hyper::service::Service<hyper::Request<hyper::body::Body>> for HyperHandler {
    type Response = hyper::Response<hyper::body::Body>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    // type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn call(&mut self, req: hyper::Request<hyper::body::Body>) -> Self::Future {
        let local_addr = self.config.local_addr.clone();
        let protocol = self.config.protocol.clone();
        let catchers = self.config.catchers.clone();
        let allowed_media_types = self.config.allowed_media_types.clone();
        let mut request = Request::from_hyper(req, local_addr, &protocol).unwrap();
        let mut response = Response::new(self.config.clone());
        let mut depot = Depot::new();

        let mut segments = request.url().path_segments().map(|c| c.collect::<Vec<_>>()).unwrap_or(Vec::new());
        segments.retain(|x| *x!="");
        let (ok, handlers, params) = self.router.detect(request.method().clone(), segments);
        if !ok {
            response.set_status_code(StatusCode::NOT_FOUND);
        }
        request.params = params;
        response.cookies = request.cookies().clone();
        let config = self.config.clone();

        let fut = async move {
            for handler in handlers{
                handler.handle(config.clone(), &mut request, &mut depot, &mut response).await;
                if response.is_commited() {
                    break;
                }
            }
            if !response.is_commited() {
                response.commit();
            }
    
            let mut hyper_response = hyper::Response::<hyper::Body>::new(hyper::Body::empty());
    
            if response.status_code().is_none(){
                if response.body_writers.len() == 0 {
                    response.set_status_code(StatusCode::NOT_FOUND);
                }else {
                    response.set_status_code(StatusCode::OK);
                }
            }
            let status = response.status_code().unwrap();
            let has_error = status.is_client_error() || status.is_server_error();
            if let Some(value) =  response.headers().get(CONTENT_TYPE) {
                let mut is_allowed = false;
                if let Ok(value) = value.to_str() {
                    if allowed_media_types.len() == 0 {
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
                warn!(logging::logger(), "Http response content type header is not set"; "url" => request.url().as_str(), "method" => request.method().as_str());
                if !has_error {
                    response.set_status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                }
            }
            if response.body_writers.len() == 0 &&  has_error {
                for catcher in &*catchers {
                    if catcher.catch(&request, &mut response) {
                        break;
                    }
                }
            }
            response.write_back(&mut hyper_response, request.method().clone());
            Ok(hyper_response)
        };
        Box::pin(fut)
    }
}