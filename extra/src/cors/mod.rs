//! CORS Filters
// modified from https://github.com/seanmonstar/salvo/blob/master/src/filters/cors.rs
use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error as StdError;
use std::sync::Arc;

use headers::{
    AccessControlAllowHeaders, AccessControlAllowMethods, AccessControlExposeHeaders, HeaderMapExt,
};
use salvo_core::http::header::{self, HeaderName, HeaderValue, HeaderMap};
use salvo_core::http::Method;

use self::internal::{IntoOrigin, Seconds};

/// [CORS]: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
///
/// # Example
///
/// ```
/// use salvo::Filter;
///
/// let cors = salvo::cors()
///     .allow_origin("https://hyper.rs")
///     .allow_methods(vec!["GET", "POST", "DELETE"]);
/// let cors = extra::cors::Handler::new().allow_origin("https://hyper.rs")
///     .allow_methods(vec!["GET", "POST", "DELETE"];
/// let router = Router::new().before(cors)).post(upload_file);
/// ```
/// If you want to allow any route:
/// ```
/// use salvo::Filter;
/// let cors = salvo::cors()
///     .allow_any_origin();
/// ```
/// You can find more usage examples [here](https://github.com/kenorld/salvo/blob/examples/cors.rs).
pub fn cors() -> Builder {
    Builder {
        credentials: false,
        allowed_headers: HashSet::new(),
        exposed_headers: HashSet::new(),
        max_age: None,
        methods: HashSet::new(),
        origins: None,
    }
}

/// A wrapping filter constructed via `salvo::cors()`.
#[derive(Clone, Debug)]
pub struct Cors {
    config: Arc<Configured>,
}

/// A constructed via `salvo::cors()`.
#[derive(Clone, Debug)]
pub struct Builder {
    credentials: bool,
    allowed_headers: HashSet<HeaderName>,
    exposed_headers: HashSet<HeaderName>,
    max_age: Option<u64>,
    methods: HashSet<Method>,
    origins: Option<HashSet<HeaderValue>>,
}

impl Builder {
    /// Sets whether to add the `Access-Control-Allow-Credentials` header.
    pub fn allow_credentials(mut self, allow: bool) -> Self {
        self.credentials = allow;
        self
    }

    /// Adds a method to the existing list of allowed request methods.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::Method`.
    pub fn allow_method<M>(mut self, method: M) -> Self
    where
        Method: TryFrom<M>,
    {
        let method = match TryFrom::try_from(method) {
            Ok(m) => m,
            Err(_) => panic!("illegal Method"),
        };
        self.methods.insert(method);
        self
    }

    /// Adds multiple methods to the existing list of allowed request methods.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::Method`.
    pub fn allow_methods<I>(mut self, methods: I) -> Self
    where
        I: IntoIterator,
        Method: TryFrom<I::Item>,
    {
        let iter = methods.into_iter().map(|m| match TryFrom::try_from(m) {
            Ok(m) => m,
            Err(_) => panic!("illegal Method"),
        });
        self.methods.extend(iter);
        self
    }

    /// Adds a header to the list of allowed request headers.
    ///
    /// **Note**: These should match the values the browser sends via `Access-Control-Request-Headers`, e.g. `content-type`.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::header::HeaderName`.
    pub fn allow_header<H>(mut self, header: H) -> Self
    where
        HeaderName: TryFrom<H>,
    {
        let header = match TryFrom::try_from(header) {
            Ok(m) => m,
            Err(_) => panic!("illegal Header"),
        };
        self.allowed_headers.insert(header);
        self
    }

    /// Adds multiple headers to the list of allowed request headers.
    ///
    /// **Note**: These should match the values the browser sends via `Access-Control-Request-Headers`, e.g.`content-type`.
    ///
    /// # Panics
    ///
    /// Panics if any of the headers are not a valid `http::header::HeaderName`.
    pub fn allow_headers<I>(mut self, headers: I) -> Self
    where
        I: IntoIterator,
        HeaderName: TryFrom<I::Item>,
    {
        let iter = headers.into_iter().map(|h| match TryFrom::try_from(h) {
            Ok(h) => h,
            Err(_) => panic!("illegal Header"),
        });
        self.allowed_headers.extend(iter);
        self
    }

    /// Adds a header to the list of exposed headers.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::header::HeaderName`.
    pub fn expose_header<H>(mut self, header: H) -> Self
    where
        HeaderName: TryFrom<H>,
    {
        let header = match TryFrom::try_from(header) {
            Ok(m) => m,
            Err(_) => panic!("illegal Header"),
        };
        self.exposed_headers.insert(header);
        self
    }

    /// Adds multiple headers to the list of exposed headers.
    ///
    /// # Panics
    ///
    /// Panics if any of the headers are not a valid `http::header::HeaderName`.
    pub fn expose_headers<I>(mut self, headers: I) -> Self
    where
        I: IntoIterator,
        HeaderName: TryFrom<I::Item>,
    {
        let iter = headers.into_iter().map(|h| match TryFrom::try_from(h) {
            Ok(h) => h,
            Err(_) => panic!("illegal Header"),
        });
        self.exposed_headers.extend(iter);
        self
    }

    /// Sets that *any* `Origin` header is allowed.
    ///
    /// # Warning
    ///
    /// This can allow websites you didn't intend to access this resource,
    /// it is usually better to set an explicit list.
    pub fn allow_any_origin(mut self) -> Self {
        self.origins = None;
        self
    }

    /// Add an origin to the existing list of allowed `Origin`s.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `Origin`.
    pub fn allow_origin(self, origin: impl IntoOrigin) -> Self {
        self.allow_origins(Some(origin))
    }

    /// Add multiple origins to the existing list of allowed `Origin`s.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `Origin`.
    pub fn allow_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: IntoOrigin,
    {
        let iter = origins
            .into_iter()
            .map(IntoOrigin::into_origin)
            .map(|origin| {
                origin
                    .to_string()
                    .parse()
                    .expect("Origin is always a valid HeaderValue")
            });

        self.origins.get_or_insert_with(HashSet::new).extend(iter);

        self
    }

    /// Sets the `Access-Control-Max-Age` header.
    ///
    /// # Example
    ///
    ///
    /// ```
    /// use std::time::Duration;
    /// use salvo::Filter;
    ///
    /// let cors = salvo::cors()
    ///     .max_age(30) // 30u32 seconds
    ///     .max_age(Duration::from_secs(30)); // or a Duration
    /// ```
    pub fn max_age(mut self, seconds: impl Seconds) -> Self {
        self.max_age = Some(seconds.seconds());
        self
    }

    /// Builds the `Cors` wrapper from the configured settings.
    ///
    /// This step isn't *required*, as the `Builder` itself can be passed
    /// to `Filter::with`. This just allows constructing once, thus not needing
    /// to pay the cost of "building" every time.
    pub fn build(self) -> Cors {
        let expose_headers_header = if self.exposed_headers.is_empty() {
            None
        } else {
            Some(self.exposed_headers.iter().cloned().collect())
        };
        let allowed_headers_header = self.allowed_headers.iter().cloned().collect();
        let methods_header = self.methods.iter().cloned().collect();

        let config = Arc::new(Configured {
            cors: self,
            allowed_headers_header,
            expose_headers_header,
            methods_header,
        });

        Cors { config }
    }
}

enum Forbidden {
    OriginNotAllowed,
    MethodNotAllowed,
    HeaderNotAllowed,
}

impl ::std::fmt::Debug for Forbidden {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_tuple("CorsForbidden").field(&self).finish()
    }
}

impl ::std::fmt::Display for Forbidden {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        let detail = match self {
            Forbidden::OriginNotAllowed => "origin not allowed",
            Forbidden::MethodNotAllowed => "request-method not allowed",
            Forbidden::HeaderNotAllowed => "header not allowed",
        };
        write!(f, "CORS request forbidden: {}", detail)
    }
}

impl StdError for Forbidden {}

#[derive(Clone, Debug)]
struct Configured {
    cors: Builder,
    allowed_headers_header: AccessControlAllowHeaders,
    expose_headers_header: Option<AccessControlExposeHeaders>,
    methods_header: AccessControlAllowMethods,
}

enum Validated {
    Preflight(HeaderValue),
    Simple(HeaderValue),
    NotCors,
}

impl Configured {
    fn check_request(
        &self,
        method: &Method,
        headers: &HeaderMap,
    ) -> Result<Validated, Forbidden> {
        match (headers.get(header::ORIGIN), method) {
            (Some(origin), &Method::OPTIONS) => {
                // OPTIONS requests are preflight CORS requests...

                if !self.is_origin_allowed(origin) {
                    return Err(Forbidden::OriginNotAllowed);
                }

                if let Some(req_method) = headers.get(header::ACCESS_CONTROL_REQUEST_METHOD) {
                    if !self.is_method_allowed(req_method) {
                        return Err(Forbidden::MethodNotAllowed);
                    }
                } else {
                    tracing::trace!(
                        "preflight request missing access-control-request-method header"
                    );
                    return Err(Forbidden::MethodNotAllowed);
                }

                if let Some(req_headers) = headers.get(header::ACCESS_CONTROL_REQUEST_HEADERS) {
                    let headers = req_headers
                        .to_str()
                        .map_err(|_| Forbidden::HeaderNotAllowed)?;
                    for header in headers.split(',') {
                        if !self.is_header_allowed(header) {
                            return Err(Forbidden::HeaderNotAllowed);
                        }
                    }
                }

                Ok(Validated::Preflight(origin.clone()))
            }
            (Some(origin), _) => {
                // Any other method, simply check for a valid origin...

                tracing::trace!("origin header: {:?}", origin);
                if self.is_origin_allowed(origin) {
                    Ok(Validated::Simple(origin.clone()))
                } else {
                    Err(Forbidden::OriginNotAllowed)
                }
            }
            (None, _) => {
                // No `ORIGIN` header means this isn't CORS!
                Ok(Validated::NotCors)
            }
        }
    }

    fn is_method_allowed(&self, header: &HeaderValue) -> bool {
        Method::from_bytes(header.as_bytes())
            .map(|method| self.cors.methods.contains(&method))
            .unwrap_or(false)
    }

    fn is_header_allowed(&self, header: &str) -> bool {
        HeaderName::from_bytes(header.as_bytes())
            .map(|header| self.cors.allowed_headers.contains(&header))
            .unwrap_or(false)
    }

    fn is_origin_allowed(&self, origin: &HeaderValue) -> bool {
        if let Some(ref allowed) = self.cors.origins {
            allowed.contains(origin)
        } else {
            true
        }
    }

    fn append_preflight_headers(&self, headers: &mut HeaderMap) {
        self.append_common_headers(headers);

        headers.typed_insert(self.allowed_headers_header.clone());
        headers.typed_insert(self.methods_header.clone());

        if let Some(max_age) = self.cors.max_age {
            headers.insert(header::ACCESS_CONTROL_MAX_AGE, max_age.into());
        }
    }

    fn append_common_headers(&self, headers: &mut HeaderMap) {
        if self.cors.credentials {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HeaderValue::from_static("true"),
            );
        }
        if let Some(expose_headers_header) = &self.expose_headers_header {
            headers.typed_insert(expose_headers_header.clone())
        }
    }
}

mod internal {
    use std::sync::Arc;

    use headers::Origin;
    use async_trait::async_trait;
    use salvo_core::http::{header, Request, Response};
    use salvo_core::{Handler, Depot};

    use super::{Configured, Validated};

    #[derive(Debug)]
    pub struct CorsHandler {
        config: Arc<Configured>,
        origin: header::HeaderValue,
    }

    #[async_trait]
    impl Handler for CorsHandler {
        async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
            let validated = self.config.check_request(req.method(), req.headers());

            match validated {
                Ok(Validated::Preflight(origin)) => {
                    self.config.append_preflight_headers(res.headers_mut());
                    res.headers_mut()
                        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
                }
                Ok(Validated::Simple(origin)) => {
                    self.config.append_common_headers(res.headers_mut());
                    res.headers_mut()
                        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
                }
                Err(err) => {
                    tracing::error!(error = %err, "CorsHandler validate error");
                }
                _ => {}
            }
        }
    }

    pub trait Seconds {
        fn seconds(self) -> u64;
    }

    impl Seconds for u32 {
        fn seconds(self) -> u64 {
            self.into()
        }
    }

    impl Seconds for ::std::time::Duration {
        fn seconds(self) -> u64 {
            self.as_secs()
        }
    }

    pub trait IntoOrigin {
        fn into_origin(self) -> Origin;
    }

    impl<'a> IntoOrigin for &'a str {
        fn into_origin(self) -> Origin {
            let mut parts = self.splitn(2, "://");
            let scheme = parts.next().expect("missing scheme");
            let rest = parts.next().expect("missing scheme");

            Origin::try_from_parts(scheme, rest, None).expect("invalid Origin")
        }
    }
}
