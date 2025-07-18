use std::error::Error as StdError;
use std::fmt::{self, Debug, Display, Formatter, Write};

use crate::http::{ResBody, StatusCode};

use crate::{Response, Scribe};

/// Result type with `StatusError` has it's error type.
pub type StatusResult<T> = Result<T, StatusError>;

macro_rules! default_errors {
    (
        $(
            $(#[$docs:meta])*
            $sname:ident, $code:expr, $name:expr, $brief:expr);
        +) =>
    {
        $(
            #[doc=concat!($brief,"\n ")]
            $(#[$docs])*
            #[must_use] pub fn $sname() -> StatusError {
                StatusError {
                    code: $code,
                    name: $name.into(),
                    brief: $brief.into(),
                    detail: None,
                    cause: None,
                    origin: None,
                }
            }
        )+
    }
}

/// HTTP status error information.
#[derive(Debug)]
#[non_exhaustive]
pub struct StatusError {
    /// Http error status code.
    pub code: StatusCode,
    /// Http error name.
    pub name: String,
    /// Brief information about http error.
    pub brief: String,
    /// Detail information about http error.
    pub detail: Option<String>,
    /// Cause about http error. Similar to the `origin` field, but using [`std::error::Error`].
    pub cause: Option<Box<dyn StdError + Sync + Send + 'static>>,
    /// Origin about http error. Similar to the `cause` field, but using [`std::any::Any`].
    pub origin: Option<Box<dyn std::any::Any + Sync + Send + 'static>>,
}

impl StatusError {
    /// Sets brief field and returns `Self`.
    #[must_use]
    pub fn brief(mut self, brief: impl Into<String>) -> Self {
        self.brief = brief.into();
        self
    }
    /// Sets detail field and returns `Self`.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets cause field and returns `Self`.
    #[must_use]
    pub fn cause<T>(mut self, cause: T) -> Self
    where
        T: Into<Box<dyn StdError + Sync + Send + 'static>>,
    {
        self.cause = Some(cause.into());
        self
    }

    /// Sets origin field and returns `Self`.
    #[must_use]
    pub fn origin<T: Send + Sync + 'static>(mut self, origin: T) -> Self {
        self.origin = Some(Box::new(origin));
        self
    }

    /// Downcast origin to T
    #[must_use]
    pub fn downcast_origin<T: 'static>(&self) -> Option<&T> {
        self.origin.as_ref().and_then(|o| o.downcast_ref::<T>())
    }

    default_errors! {
        /// 400 Bad Request
        /// [[RFC7231, Section 6.5.1](https://tools.ietf.org/html/rfc7231#section-6.5.1)]
        bad_request,                        StatusCode::BAD_REQUEST,            "Bad Request", "The request could not be understood by the server due to malformed syntax.";
        /// 401 Unauthorized
        /// [[RFC7235, Section 3.1](https://tools.ietf.org/html/rfc7235#section-3.1)]
        unauthorized,                       StatusCode::UNAUTHORIZED,           "Unauthorized", "The request requires user authentication.";
        /// 402 Payment Required
        /// [[RFC7231, Section 6.5.2](https://tools.ietf.org/html/rfc7231#section-6.5.2)]
        payment_required,                   StatusCode::PAYMENT_REQUIRED,       "Payment Required", "The request could not be processed due to lack of payment.";
        /// 403 Forbidden
        /// [[RFC7231, Section 6.5.3](https://tools.ietf.org/html/rfc7231#section-6.5.3)]
        forbidden,                          StatusCode::FORBIDDEN,              "Forbidden", "The server refused to authorize the request.";
        /// 404 Not Found
        /// [[RFC7231, Section 6.5.4](https://tools.ietf.org/html/rfc7231#section-6.5.4)]
        not_found,                          StatusCode::NOT_FOUND,              "Not Found", "The requested resource could not be found.";
        /// 405 Method Not Allowed
        /// [[RFC7231, Section 6.5.5](https://tools.ietf.org/html/rfc7231#section-6.5.5)]
        method_not_allowed,                 StatusCode::METHOD_NOT_ALLOWED,     "Method Not Allowed", "The request method is not supported for the requested resource.";
        /// 406 Not Acceptable
        /// [[RFC7231, Section 6.5.6](https://tools.ietf.org/html/rfc7231#section-6.5.6)]
        not_acceptable,                     StatusCode::NOT_ACCEPTABLE,         "Not Acceptable", "The requested resource is capable of generating only content not acceptable according to the Accept headers sent in the request.";
        /// 407 Proxy Authentication Required
        /// [[RFC7235, Section 3.2](https://tools.ietf.org/html/rfc7235#section-3.2)]
        proxy_authentication_required,      StatusCode::PROXY_AUTHENTICATION_REQUIRED,  "Proxy Authentication Required", "Authentication with the proxy is required.";
        /// 408 Request Timeout
        /// [[RFC7231, Section 6.5.7](https://tools.ietf.org/html/rfc7231#section-6.5.7)]
        request_timeout,                    StatusCode::REQUEST_TIMEOUT,        "Request Timeout", "The server timed out waiting for the request.";
        /// 409 Conflict
        /// [[RFC7231, Section 6.5.8](https://tools.ietf.org/html/rfc7231#section-6.5.8)]
        conflict,                           StatusCode::CONFLICT,               "Conflict", "The request could not be processed because of a conflict in the request.";
        /// 410 Gone
        /// [[RFC7231, Section 6.5.9](https://tools.ietf.org/html/rfc7231#section-6.5.9)]
        gone,                               StatusCode::GONE,                   "Gone", "The resource requested is no longer available and will not be available again.";
        /// 411 Length Required
        /// [[RFC7231, Section 6.5.10](https://tools.ietf.org/html/rfc7231#section-6.5.10)]
        length_required,                    StatusCode::LENGTH_REQUIRED,        "Length Required", "The request did not specify the length of its content, which is required by the requested resource.";
        /// 412 Precondition Failed
        /// [[RFC7232, Section 4.2](https://tools.ietf.org/html/rfc7232#section-4.2)]
        precondition_failed,                StatusCode::PRECONDITION_FAILED,    "Precondition Failed", "The server does not meet one of the preconditions specified in the request.";
        /// 413 Payload Too Large
        /// [[RFC7231, Section 6.5.11](https://tools.ietf.org/html/rfc7231#section-6.5.11)]
        payload_too_large,                  StatusCode::PAYLOAD_TOO_LARGE,      "Payload Too Large", "The request is larger than the server is willing or able to process.";
        /// 414 URI Too Long
        /// [[RFC7231, Section 6.5.12](https://tools.ietf.org/html/rfc7231#section-6.5.12)]
        uri_too_long,                       StatusCode::URI_TOO_LONG,           "URI Too Long", "The URI provided was too long for the server to process.";
        /// 415 Unsupported Media Type
        /// [[RFC7231, Section 6.5.13](https://tools.ietf.org/html/rfc7231#section-6.5.13)]
        unsupported_media_type,             StatusCode::UNSUPPORTED_MEDIA_TYPE, "Unsupported Media Type", "The request entity has a media type which the server or resource does not support.";
        /// 416 Range Not Satisfiable
        /// [[RFC7233, Section 4.4](https://tools.ietf.org/html/rfc7233#section-4.4)]
        range_not_satisfiable,              StatusCode::RANGE_NOT_SATISFIABLE,  "Range Not Satisfiable", "The portion of the requested file cannot be supplied by the server.";
        /// 417 Expectation Failed
        /// [[RFC7231, Section 6.5.14](https://tools.ietf.org/html/rfc7231#section-6.5.14)]
        expectation_failed,                 StatusCode::EXPECTATION_FAILED,     "Expectation Failed", "The server cannot meet the requirements of the expect request-header field.";
        /// 418 I'm a teapot
        /// [curiously not registered by IANA but [RFC2324](https://tools.ietf.org/html/rfc2324)]
        im_a_teapot,                        StatusCode::IM_A_TEAPOT,            "I'm a teapot", "I was requested to brew coffee, and I am a teapot.";
        /// 421 Misdirected Request
        /// [RFC7540, Section 9.1.2](https://tools.ietf.org/html/rfc7540#section-9.1.2)
        misdirected_request,                StatusCode::MISDIRECTED_REQUEST,    "Misdirected Request", "The server cannot produce a response for this request.";
        /// 422 Unprocessable Entity
        /// [[RFC4918](https://tools.ietf.org/html/rfc4918)]
        unprocessable_entity,               StatusCode::UNPROCESSABLE_ENTITY,   "Unprocessable Entity", "The request was well-formed but was unable to be followed due to semantic errors.";
        /// 423 Locked
        /// [[RFC4918](https://tools.ietf.org/html/rfc4918)]
        locked,                             StatusCode::LOCKED,                 "Locked", "The source or destination resource of a method is locked.";
        /// 424 Failed Dependency
        /// [[RFC4918](https://tools.ietf.org/html/rfc4918)]
        failed_dependency,                  StatusCode::FAILED_DEPENDENCY,      "Failed Dependency", "The method could not be performed on the resource because the requested action depended on another action and that action failed.";
        /// 426 Upgrade Required
        /// [[RFC7231, Section 6.5.15](https://tools.ietf.org/html/rfc7231#section-6.5.15)]
        upgrade_required,                   StatusCode::UPGRADE_REQUIRED,       "Upgrade Required", "Switching to the protocol in the Upgrade header field is required.";
        /// 428 Precondition Required
        /// [[RFC6585](https://tools.ietf.org/html/rfc6585)]
        precondition_required,              StatusCode::PRECONDITION_REQUIRED,  "Precondition Required", "The server requires the request to be conditional.";
        /// 429 Too Many Requests
        /// [[RFC6585](https://tools.ietf.org/html/rfc6585)]
        too_many_requests,                  StatusCode::TOO_MANY_REQUESTS,      "Too Many Requests", "Too many requests have been received recently.";
        /// 431 Request Header Fields Too Large
        /// [[RFC6585](https://tools.ietf.org/html/rfc6585)]
        request_header_fields_toolarge,     StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,    "Request Header Fields Too Large", "The server is unwilling to process the request because either  an individual header field, or all the header fields collectively, are too large.";
         /// 451 Unavailable For Legal Reasons
         /// [[RFC7725](https://tools.ietf.org/html/rfc7725)]
        unavailable_for_legalreasons,       StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,      "Unavailable For Legal Reasons", "The requested resource is unavailable due to a legal demand to deny access to this resource.";
        /// 500 Internal Server Error
        /// [[RFC7231, Section 6.6.1](https://tools.ietf.org/html/rfc7231#section-6.6.1)]
        internal_server_error,              StatusCode::INTERNAL_SERVER_ERROR,  "Internal Server Error", "The server encountered an internal error while processing this request.";
        /// 501 Not Implemented
        /// [[RFC7231, Section 6.6.2](https://tools.ietf.org/html/rfc7231#section-6.6.2)]
        not_implemented,                    StatusCode::NOT_IMPLEMENTED,        "Not Implemented", "The server either does not recognize the request method, or it lacks the ability to fulfill the request.";
        /// 502 Bad Gateway
        /// [[RFC7231, Section 6.6.3](https://tools.ietf.org/html/rfc7231#section-6.6.3)]
        bad_gateway,                        StatusCode::BAD_GATEWAY,            "Bad Gateway", "Received an invalid response from an inbound server it accessed while attempting to fulfill the request.";
        /// 503 Service Unavailable
        /// [[RFC7231, Section 6.6.4](https://tools.ietf.org/html/rfc7231#section-6.6.4)]
        service_unavailable,                StatusCode::SERVICE_UNAVAILABLE,    "Service Unavailable", "The server is currently unavailable.";
        /// 504 Gateway Timeout
        /// [[RFC7231, Section 6.6.5](https://tools.ietf.org/html/rfc7231#section-6.6.5)]
        gateway_timeout,                    StatusCode::GATEWAY_TIMEOUT,        "Gateway Timeout", "The server did not receive a timely response from an upstream server.";
        /// 505 HTTP Version Not Supported
        /// [[RFC7231, Section 6.6.6](https://tools.ietf.org/html/rfc7231#section-6.6.6)]
        http_version_not_supported,         StatusCode::HTTP_VERSION_NOT_SUPPORTED, "HTTP Version Not Supported", "The server does not support, or refuses to support, the major version of HTTP that was used in the request message.";
        /// 506 Variant Also Negotiates
        /// [[RFC2295](https://tools.ietf.org/html/rfc2295)]
        variant_also_negotiates,            StatusCode::VARIANT_ALSO_NEGOTIATES, "Variant Also Negotiates", "The server has an internal configuration error.";
        /// 507 Insufficient Storage
        /// [[RFC4918](https://tools.ietf.org/html/rfc4918)]
        insufficient_storage,               StatusCode::INSUFFICIENT_STORAGE,    "Insufficient Storage", "The method could not be performed on the resource because the server is unable to store the representation needed to successfully complete the request.";
        /// 508 Loop Detected
        /// [[RFC5842](https://tools.ietf.org/html/rfc5842)]
        loop_detected,                      StatusCode::LOOP_DETECTED,           "Loop Detected", "the server terminated an operation because it encountered an infinite loop while processing a request with \"Depth: infinity\".";
        /// 510 Not Extended
        /// [[RFC2774](https://tools.ietf.org/html/rfc2774)]
        not_extended,                       StatusCode::NOT_EXTENDED,            "Not Extended", "Further extensions to the request are required for the server to fulfill it.";
        /// 511 Network Authentication Required
        /// [[RFC6585](https://tools.ietf.org/html/rfc6585)]
        network_authentication_required,    StatusCode::NETWORK_AUTHENTICATION_REQUIRED, "Network Authentication Required", "the client needs to authenticate to gain network access."
    }
}

impl StdError for StatusError {}

impl Display for StatusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut str_error = format!(
            "code: {} name: {} brief: {}",
            self.code, self.name, self.brief
        );
        if let Some(detail) = &self.detail {
            write!(&mut str_error, " detail: {detail}")?;
        }
        if let Some(cause) = &self.cause {
            write!(&mut str_error, " cause: {cause}")?;
        }
        if let Some(origin) = &self.origin {
            let mut handle_cause = || {
                if let Some(e) = origin.downcast_ref::<&dyn StdError>() {
                    return write!(&mut str_error, " origin: {e}");
                }
                if let Some(e) = origin.downcast_ref::<String>() {
                    return write!(&mut str_error, " origin: {e}");
                }
                if let Some(e) = origin.downcast_ref::<&str>() {
                    return write!(&mut str_error, " origin: {e}");
                }
                #[cfg(feature = "anyhow")]
                if let Some(e) = origin.downcast_ref::<anyhow::Error>() {
                    return write!(&mut str_error, " origin: {e}");
                }
                write!(&mut str_error, " origin: <unknown error type>")
            };
            handle_cause()?;
        }
        f.write_str(&str_error)
    }
}

impl StatusError {
    /// Create new `StatusError` with code. If code is not error, it will be `None`.
    #[must_use]
    pub fn from_code(code: StatusCode) -> Option<Self> {
        match code {
            StatusCode::BAD_REQUEST => Some(Self::bad_request()),
            StatusCode::UNAUTHORIZED => Some(Self::unauthorized()),
            StatusCode::PAYMENT_REQUIRED => Some(Self::payment_required()),
            StatusCode::FORBIDDEN => Some(Self::forbidden()),
            StatusCode::NOT_FOUND => Some(Self::not_found()),
            StatusCode::METHOD_NOT_ALLOWED => Some(Self::method_not_allowed()),
            StatusCode::NOT_ACCEPTABLE => Some(Self::not_acceptable()),
            StatusCode::PROXY_AUTHENTICATION_REQUIRED => {
                Some(Self::proxy_authentication_required())
            }
            StatusCode::REQUEST_TIMEOUT => Some(Self::request_timeout()),
            StatusCode::CONFLICT => Some(Self::conflict()),
            StatusCode::GONE => Some(Self::gone()),
            StatusCode::LENGTH_REQUIRED => Some(Self::length_required()),
            StatusCode::PRECONDITION_FAILED => Some(Self::precondition_failed()),
            StatusCode::PAYLOAD_TOO_LARGE => Some(Self::payload_too_large()),
            StatusCode::URI_TOO_LONG => Some(Self::uri_too_long()),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Some(Self::unsupported_media_type()),
            StatusCode::RANGE_NOT_SATISFIABLE => Some(Self::range_not_satisfiable()),
            StatusCode::EXPECTATION_FAILED => Some(Self::expectation_failed()),
            StatusCode::IM_A_TEAPOT => Some(Self::im_a_teapot()),
            StatusCode::MISDIRECTED_REQUEST => Some(Self::misdirected_request()),
            StatusCode::UNPROCESSABLE_ENTITY => Some(Self::unprocessable_entity()),
            StatusCode::LOCKED => Some(Self::locked()),
            StatusCode::FAILED_DEPENDENCY => Some(Self::failed_dependency()),
            StatusCode::UPGRADE_REQUIRED => Some(Self::upgrade_required()),
            StatusCode::PRECONDITION_REQUIRED => Some(Self::precondition_required()),
            StatusCode::TOO_MANY_REQUESTS => Some(Self::too_many_requests()),
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE => {
                Some(Self::request_header_fields_toolarge())
            }
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => Some(Self::unavailable_for_legalreasons()),
            StatusCode::INTERNAL_SERVER_ERROR => Some(Self::internal_server_error()),
            StatusCode::NOT_IMPLEMENTED => Some(Self::not_implemented()),
            StatusCode::BAD_GATEWAY => Some(Self::bad_gateway()),
            StatusCode::SERVICE_UNAVAILABLE => Some(Self::service_unavailable()),
            StatusCode::GATEWAY_TIMEOUT => Some(Self::gateway_timeout()),
            StatusCode::HTTP_VERSION_NOT_SUPPORTED => Some(Self::http_version_not_supported()),
            StatusCode::VARIANT_ALSO_NEGOTIATES => Some(Self::variant_also_negotiates()),
            StatusCode::INSUFFICIENT_STORAGE => Some(Self::insufficient_storage()),
            StatusCode::LOOP_DETECTED => Some(Self::loop_detected()),
            StatusCode::NOT_EXTENDED => Some(Self::not_extended()),
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED => {
                Some(Self::network_authentication_required())
            }
            _ => None,
        }
    }
}

impl Scribe for StatusError {
    #[inline]
    fn render(self, res: &mut Response) {
        res.status_code = Some(self.code);
        res.body = ResBody::Error(self);
    }
}
