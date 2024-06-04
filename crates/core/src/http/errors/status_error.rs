use std::error::Error as StdError;
use std::fmt::{self, Debug, Display, Formatter};

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
            pub fn $sname() -> StatusError {
                StatusError {
                    code: $code,
                    name: $name.into(),
                    brief: $brief.into(),
                    detail: None,
                    cause: None,
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
    /// Cause about http error. This field is only used for internal debugging and only used in debug mode.
    pub cause: Option<Box<dyn StdError + Sync + Send + 'static>>,
}

impl StatusError {
    /// Sets brief field and returns `Self`.
    pub fn brief(mut self, brief: impl Into<String>) -> Self {
        self.brief = brief.into();
        self
    }
    /// Sets detail field and returns `Self`.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    /// Sets cause field and returns `Self`.
    pub fn cause<C>(mut self, cause: C) -> Self
    where
        C: Into<Box<dyn StdError + Sync + Send + 'static>>,
    {
        self.cause = Some(cause.into());
        self
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
        write!(f, "code: {}", self.code)?;
        write!(f, "name: {}", self.name)?;
        write!(f, "brief: {:?}", self.brief)?;
        write!(f, "detail: {:?}", self.detail)?;
        write!(f, "cause: {:?}", self.cause)?;
        Ok(())
    }
}

impl StatusError {
    /// Create new `StatusError` with code. If code is not error, it will be `None`.
    pub fn from_code(code: StatusCode) -> Option<StatusError> {
        match code {
            StatusCode::BAD_REQUEST => Some(StatusError::bad_request()),
            StatusCode::UNAUTHORIZED => Some(StatusError::unauthorized()),
            StatusCode::PAYMENT_REQUIRED => Some(StatusError::payment_required()),
            StatusCode::FORBIDDEN => Some(StatusError::forbidden()),
            StatusCode::NOT_FOUND => Some(StatusError::not_found()),
            StatusCode::METHOD_NOT_ALLOWED => Some(StatusError::method_not_allowed()),
            StatusCode::NOT_ACCEPTABLE => Some(StatusError::not_acceptable()),
            StatusCode::PROXY_AUTHENTICATION_REQUIRED => Some(StatusError::proxy_authentication_required()),
            StatusCode::REQUEST_TIMEOUT => Some(StatusError::request_timeout()),
            StatusCode::CONFLICT => Some(StatusError::conflict()),
            StatusCode::GONE => Some(StatusError::gone()),
            StatusCode::LENGTH_REQUIRED => Some(StatusError::length_required()),
            StatusCode::PRECONDITION_FAILED => Some(StatusError::precondition_failed()),
            StatusCode::PAYLOAD_TOO_LARGE => Some(StatusError::payload_too_large()),
            StatusCode::URI_TOO_LONG => Some(StatusError::uri_too_long()),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Some(StatusError::unsupported_media_type()),
            StatusCode::RANGE_NOT_SATISFIABLE => Some(StatusError::range_not_satisfiable()),
            StatusCode::EXPECTATION_FAILED => Some(StatusError::expectation_failed()),
            StatusCode::IM_A_TEAPOT => Some(StatusError::im_a_teapot()),
            StatusCode::MISDIRECTED_REQUEST => Some(StatusError::misdirected_request()),
            StatusCode::UNPROCESSABLE_ENTITY => Some(StatusError::unprocessable_entity()),
            StatusCode::LOCKED => Some(StatusError::locked()),
            StatusCode::FAILED_DEPENDENCY => Some(StatusError::failed_dependency()),
            StatusCode::UPGRADE_REQUIRED => Some(StatusError::upgrade_required()),
            StatusCode::PRECONDITION_REQUIRED => Some(StatusError::precondition_required()),
            StatusCode::TOO_MANY_REQUESTS => Some(StatusError::too_many_requests()),
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE => Some(StatusError::request_header_fields_toolarge()),
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => Some(StatusError::unavailable_for_legalreasons()),
            StatusCode::INTERNAL_SERVER_ERROR => Some(StatusError::internal_server_error()),
            StatusCode::NOT_IMPLEMENTED => Some(StatusError::not_implemented()),
            StatusCode::BAD_GATEWAY => Some(StatusError::bad_gateway()),
            StatusCode::SERVICE_UNAVAILABLE => Some(StatusError::service_unavailable()),
            StatusCode::GATEWAY_TIMEOUT => Some(StatusError::gateway_timeout()),
            StatusCode::HTTP_VERSION_NOT_SUPPORTED => Some(StatusError::http_version_not_supported()),
            StatusCode::VARIANT_ALSO_NEGOTIATES => Some(StatusError::variant_also_negotiates()),
            StatusCode::INSUFFICIENT_STORAGE => Some(StatusError::insufficient_storage()),
            StatusCode::LOOP_DETECTED => Some(StatusError::loop_detected()),
            StatusCode::NOT_EXTENDED => Some(StatusError::not_extended()),
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED => Some(StatusError::network_authentication_required()),
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
