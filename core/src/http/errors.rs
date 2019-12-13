use std::any::{Any, TypeId};
use std::error::Error as StdError;
use std::fmt;
use crate::http::StatusCode;

pub trait HttpError: Send + Sync + Sendfmt::Display + fmt::Debug + 'static {
    fn code(&self) -> StatusCode;
    fn name(&self) -> &str;
    fn summary(&self) -> &str;
    fn detail(&self) -> &str;
    fn get_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl dyn HttpError {
    pub fn is<T: Any>(&self) -> bool {
        self.get_type_id() == TypeId::of::<T>()
    }

    pub fn from_std_error(err: Box<dyn StdError + Send>) -> Box<dyn HttpError> {
        InternalServerError::new("Internal Server Error", format!("{}", err))
    }
}

impl HttpError for Box<dyn HttpError> {
    fn code(&self) -> StatusCode {
        (**self).code()
    }
    fn name(&self) -> &str {
        (**self).name()
    }
    fn summary(&self) -> &str {
        (**self).summary()
    }
    fn detail(&self) -> &str {
        (**self).detail()
    }
}

pub type HttpResult<T> = Result<T, Box<dyn HttpError>>;

impl<E: StdError + Send + 'static> HttpError for E {
    fn code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
    fn name(&self) -> &str {
        "INTERNAL_SERVER_ERROR"
    }
    fn summary(&self) -> &str {
        "Internal Server Error"
    }
    fn detail(&self) -> &str {
        "The server encountered an internal error while processing this request."
    }
}

impl<E: StdError + Send + 'static> From<E> for Box<dyn HttpError> {
    fn from(err: E) -> Box<dyn HttpError> {
        InternalServerError::new("Internal Server Error", format!("{}", err))
    }
}

#[derive(Debug)]
struct ConcreteError {
    code: StatusCode,
    name: String,
    summary: Option<String>,
    detail: Option<String>,
}

impl fmt::Display for ConcreteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref s) = self.summary {
            write!(f, " ({})", s)?;
        }
        if let Some(ref s) = self.detail {
            write!(f, " ({})", s)?;
        }
        Ok(())
    }
}


impl HttpError for ConcreteError {
    fn code(&self) -> StatusCode {
        self.code
    }
    fn name(&self) -> &str {
        self.name.as_str()
    }
    fn summary(&self) -> &str {
        if let Some(summary) = self.summary {
            summary.as_str()
        } else {
            ""
        }
    }
    fn detail(&self) -> &str {
        if let Some(detail) = self.detail {
            detail.as_str()
        } else {
            ""
        }
    }
}

macro_rules! default_errors {
    ($($sname:ident, $code:expr, $name:expr, $summary:expr, $detail:expr),+) => (
        $(
            #[derive(Debug, Clone)]
            pub struct $sname(String, String);
            impl $sname {
                pub fn new<S, D>(summary: S, detail: D) -> $sname where S: Into<String>, D: Into<String>{
                    $sname(summary.into(), detail.into())
                }
                pub fn with_default() -> $sname {
                    $sname($summary.into(), $detail.into())
                }
            }
            impl HttpError for $sname {
                fn code(&self) -> StatusCode {
                    $code
                }

                fn name(&self) -> &str {
                    $name
                }
                fn summary(&self) -> &str {
                    self.0.as_str()
                }
                fn detail(&self) -> &str {
                    self.1.as_str()
                }
            }

            impl fmt::Display for $sname {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    $name.fmt(f)
                }
            }
        )+
    )
}

default_errors! {
    BadRequestError,            StatusCode::BAD_REQUEST,            "BAD_REQUEST", "Bad Request", "The request could not be understood by the server due to malformed syntax.", 
    UnauthorizedError,          StatusCode::UNAUTHORIZED,           "UNAUTHORIZED", "Unauthorized", "The request requires user authentication.",
    PaymentRequiredError,       StatusCode::PAYMENT_REQUIRED,       "PAYMENT_REQUIRED", "Payment Required", "The request could not be processed due to lack of payment.",
    ForbiddenError,             StatusCode::FORBIDDEN,              "FORBIDDEN", "Forbidden", "The server refused to authorize the request.",
    NotFoundError,              StatusCode::NOT_FOUND,              "NOT_FOUND", "Not Found", "The requested resource could not be found.",
    MethodNotAllowedError,      StatusCode::METHOD_NOT_ALLOWED,     "METHOD_NOT_ALLOWED", "Method Not Allowed", "The request method is not supported for the requested resource.",
    NotAcceptableError,         StatusCode::NOT_ACCEPTABLE,         "NOT_ACCEPTABLE", "Not Acceptable", "The requested resource is capable of generating only content not acceptable according to the Accept headers sent in the request.",
    ProxyAuthenticationRequiredError, StatusCode::PROXY_AUTHENTICATION_REQUIRED,  "PROXY_AUTHENTICATION_REQUIRED", "Proxy Authentication Required", "Authentication with the proxy is required.", 
    RequestTimeoutError,        StatusCode::REQUEST_TIMEOUT,        "REQUEST_TIMEOUT", "Request Timeout", "The server timed out waiting for the request.",
    ConflictError,              StatusCode::CONFLICT,               "CONFLICT", "Conflict", "The request could not be processed because of a conflict in the request.",
    GoneError,                  StatusCode::GONE,                   "GONE", "Gone", "The resource requested is no longer available and will not be available again.",
    LengthRequiredError,        StatusCode::LENGTH_REQUIRED,        "LENGTH_REQUIRED", "Length Required", "The request did not specify the length of its content, which is required by the requested resource.",
    PreconditionFailedError,    StatusCode::PRECONDITION_FAILED,    "PRECONDITION_FAILED", "Precondition Failed", "The server does not meet one of the preconditions specified in the request.",
    PayloadTooLargeError,       StatusCode::PAYLOAD_TOO_LARGE,      "PAYLOAD_TOO_LARGE", "Payload Too Large", "The request is larger than the server is willing or able to process.",
    UriTooLongError,            StatusCode::URI_TOO_LONG,           "URI_TOO_LONG", "URI Too Long", "The URI provided was too long for the server to process.",
    UnsupportedMediaTypeError,  StatusCode::UNSUPPORTED_MEDIA_TYPE, "UNSUPPORTED_MEDIA_TYPE", "Unsupported Media Type", "The request entity has a media type which the server or resource does not support.",
    RangeNotSatisfiableError,   StatusCode::RANGE_NOT_SATISFIABLE,  "RANGE_NOT_SATISFIABLE", "Range Not Satisfiable", "The portion of the requested file cannot be supplied by the server.",
    ExpectationFailedError,     StatusCode::EXPECTATION_FAILED,     "EXPECTATION_FAILED", "Expectation Failed", "The server cannot meet the requirements of the expect request-header field.",
    ImATeapotError,             StatusCode::IM_A_TEAPOT,            "IM_A_TEAPOT", "I'm a teapot", "I was requested to brew coffee, and I am a teapot.",
    MisdirectedRequestError,    StatusCode::MISDIRECTED_REQUEST,    "MISDIRECTED_REQUEST", "Misdirected Request", "The server cannot produce a response for this request.",
    UnprocessableEntityError,   StatusCode::UNPROCESSABLE_ENTITY,   "UNPROCESSABLE_ENTITY", "Unprocessable Entity", "The request was well-formed but was unable to be followed due to semantic errors.",
    LockedError,                StatusCode::LOCKED,                 "LOCKED", "Locked", "The source or destination resource of a method is locked.",
    FailedDependencyError,      StatusCode::FAILED_DEPENDENCY,      "FAILED_DEPENDENCY", "Failed Dependency", "The method could not be performed on the resource because the requested action depended on another action and that action failed.",
    UpgradeRequiredError,       StatusCode::UPGRADE_REQUIRED,       "UPGRADE_REQUIRED", "Upgrade Required", "Switching to the protocol in the Upgrade header field is required.",
    PreconditionRequiredError,  StatusCode::PRECONDITION_REQUIRED,  "PRECONDITION_REQUIRED", "Precondition Required", "The server requires the request to be conditional.",
    TooManyRequestsError,       StatusCode::TOO_MANY_REQUESTS,      "TOO_MANY_REQUESTS", "Too Many Requests", "Too many requests have been received recently.",
    RequestHeaderFieldsTooLargeError,   StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,    "REQUEST_HEADER_FIELDS_TOO_LARGE", "Request Header Fields Too Large", "The server is unwilling to process the request because either  an individual header field, or all the header fields collectively, are too large.",
    UnavailableForLegalReasonsError,    StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,      "UNAVAILABLE_FOR_LEGAL_REASONS", "Unavailable For Legal Reasons", "The requested resource is unavailable due to a legal demand to deny access to this resource.", 
    InternalServerError,        StatusCode::INTERNAL_SERVER_ERROR,  "INTERNAL_SERVER_ERROR", "Internal Server Error", "The server encountered an internal error while processing this request.",
    NotImplementedError,        StatusCode::NOT_IMPLEMENTED,        "NOT_IMPLEMENTED", "Not Implemented", "The server either does not recognize the request method, or it lacks the ability to fulfill the request.",
    BadGatewayError,            StatusCode::BAD_GATEWAY,            "BAD_GATEWAY", "Bad Gateway", "Received an invalid response from an inbound server it accessed while attempting to fulfill the request.",
    ServiceUnavailableError,    StatusCode::SERVICE_UNAVAILABLE,    "SERVICE_UNAVAILABLE", "Service Unavailable", "The server is currently unavailable.",
    GatewayTimeoutError,        StatusCode::GATEWAY_TIMEOUT,        "GATEWAY_TIMEOUT", "Gateway Timeout", "The server did not receive a timely response from an upstream server.",
    HttpVersionNotSupportedError, StatusCode::HTTP_VERSION_NOT_SUPPORTED, "HTTP_VERSION_NOT_SUPPORTED", "HTTP Version Not Supported", "The server does not support, or refuses to support, the major version of HTTP that was used in the request message.",
    VariantAlsoNegotiatesError, StatusCode::VARIANT_ALSO_NEGOTIATES, "VARIANT_ALSO_NEGOTIATES", "Variant Also Negotiates", "The server has an internal configuration error.",
    InsufficientStorageError,   StatusCode::INSUFFICIENT_STORAGE,    "INSUFFICIENT_STORAGE", "Insufficient Storage", "The method could not be performed on the resource because the server is unable to store the representation needed to successfully complete the request.",
    LoopDetectedError,          StatusCode::LOOP_DETECTED,           "LOOP_DETECTED", "Loop Detected", "the server terminated an operation because it encountered an infinite loop while processing a request with \"Depth: infinity\".",
    NotExtendedError,           StatusCode::NOT_EXTENDED,            "NOT_EXTENDED", "Not Extended", "Further extensions to the request are required for the server to fulfill it.",
    NetworkAuthenticationRequiredError, StatusCode::NETWORK_AUTHENTICATION_REQUIRED, "NETWORK_AUTHENTICATION_REQUIRED", "Network Authentication Required", "the client needs to authenticate to gain network access."
}