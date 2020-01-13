use std::sync::Arc;
use std::time::Duration;

use hyper::Server as HyperServer;
use futures_cpupool::CpuPool;

use crate::{Protocol, Catcher, Depot};
use crate::http::{StatusCode, Request, Response, Mime};
use crate::http::headers::{CONTENT_TYPE, SET_COOKIE};
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

    /// Cpu pool to run synchronus requests on.
    ///
    /// Defaults to `num_cpus`.  Note that reading/writing to the client is
    /// handled asyncronusly in a single thread.
    pub pool: CpuPool,

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
        let mimes = vec![
            mime::TEXT_PLAIN,
            mime::TEXT_HTML,
            mime::TEXT_CSS,
            mime::TEXT_JAVASCRIPT,
            mime::TEXT_XML,
            mime::TEXT_EVENT_STREAM,
            mime::TEXT_CSV,
            mime::TEXT_VCARD,
            mime::IMAGE_JPEG,
            mime::IMAGE_GIF,
            mime::IMAGE_PNG,
            mime::IMAGE_BMP,
            mime::IMAGE_SVG,
            mime::FONT_WOFF,
            mime::FONT_WOFF2,
            mime::APPLICATION_JSON,
            mime::APPLICATION_JAVASCRIPT,
            mime::APPLICATION_OCTET_STREAM,
            mime::APPLICATION_MSGPACK,
            mime::APPLICATION_PDF,
        ];
        ServerConfig{
            protocol: Protocol::http(),
            local_addr: None,
            timeouts: Timeouts::default(),
            pool: CpuPool::new_num_cpus(),
            catchers: Arc::new(catcher::defaults::get()),
            allowed_media_types: Arc::new(mimes),
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

    pub fn serve(self) -> impl Future<Item=(), Error=()> + Send + 'static {
        let addr: SocketAddr = self.config.local_addr.unwrap_or_else(|| {
            let port = pick_port::pick_unused_port().expect("Pick unused port failed");
            let addr = format!("localhost:{}", port).to_socket_addrs().unwrap().next().unwrap();
            warn!(logging::logger(), "Local address is not set, randrom address used.");
            addr
        });
        info!(logging::logger(), "Server will be served"; "address" => addr);
        // Arc::get_mut(&mut self.config).unwrap().local_addr = Some(addr);
        HyperServer::bind(&addr)
            .tcp_keepalive(self.config.timeouts.keep_alive)
            .serve(self).map_err(|e| eprintln!("server error: {}", e))
    }
}
impl hyper::service::NewService for Server {
    type ReqBody = hyper::body::Body;
    type ResBody = hyper::body::Body;
    type Error = hyper::Error;
    type Service = HyperHandler;
    type InitError = hyper::Error;
    type Future = future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        future::ok(HyperHandler {
            handler: self.router.clone(),
            server_config: self.config.clone(),
         })
    }
}
pub struct HyperHandler {
    handler: Arc<Router>,
    server_config: Arc<ServerConfig>,
}

impl hyper::service::Service for HyperHandler {
    type ReqBody = hyper::body::Body;
    type ResBody = hyper::body::Body;
    type Error = hyper::Error;
    type Future = Box<dyn Future<Item = hyper::Response<Self::ResBody>, Error = Self::Error> + Send>;
    
    fn call(&mut self, req: hyper::Request<Self::ReqBody>) -> Self::Future {
        let handler = self.handler.clone();
        let sconfig = self.server_config.clone();

        let pool = sconfig.pool.clone();
        let local_addr = sconfig.local_addr.clone();
        let protocol = sconfig.protocol.clone();
        let catchers = sconfig.catchers.clone();
        let allowed_media_types = sconfig.allowed_media_types.clone();
        Box::new(pool.spawn_fn(move || {
            let mut request = Request::from_hyper(req, local_addr, &protocol).unwrap();
            let mut response = Response::new(sconfig);
            let mut depot = Depot::new();

            let mut segments = request.url().path_segments().map(|c| c.collect::<Vec<_>>()).unwrap_or(Vec::new());
            segments.retain(|x| *x!="");
            let (ok, handlers, params) = self.handler.detect(request.method().clone(), segments);
            if !ok {
                response.set_status_code(StatusCode::NOT_FOUND);
            }
            request.params = params;
            response.cookies = request.cookies().clone();
            for handler in handlers{
                handler.handle(sconfig, &request, &mut depot, &mut response);
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
            let has_error = status.as_str().starts_with('4') || status.as_str().starts_with('5');
            if let Some(value) =  response.headers().get(CONTENT_TYPE) {
                let mut is_allowed = false;
                if let Ok(value) = value.to_str() {
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
                if !is_allowed {
                    response.set_status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                }
            } else {
                warn!(logging::logger(), "Http response content type header is not set"; "url" => request.url().as_str(), "method" => request.method().as_str());
                if !has_error {
                    response.set_status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                }
            }
            if response.body_writers.len() == 0 &&  has_error{
                for catcher in &*catchers {
                    if catcher.catch(&request, &mut response){
                        break;
                    }
                }
            }
            for cookie in response.cookies.delta() {
                if let Ok(hv) = cookie.encoded().to_string().parse(){
                    response.headers_mut().append(SET_COOKIE, hv);
                }
            }
            response.write_back(&mut hyper_response, request.method().clone());
            future::ok(hyper_response)
        }))
    }
}