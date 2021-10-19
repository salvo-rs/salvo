use std::error::Error as StdError;
use std::fmt;

use async_trait::async_trait;
use http::StatusCode;
use mime::Mime;
use once_cell::sync::Lazy;

use crate::{Depot, Request, Response, Writer};

static SUPPORTED_FORMATS: Lazy<Vec<mime::Name>> = Lazy::new(||vec![mime::JSON, mime::HTML, mime::XML, mime::TEXT]);

fn error_html(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width">
    <title>{0}: {1}</title>
    <style>
    :root {{
        --bg-color: #fff;
        --text-color: #222;
    }}
    body {{
        background: var(--bg-color);
        color: var(--text-color);
        text-align: center;
    }}
    footer{{text-align:center;font-size:12px;}}
    @media (prefers-color-scheme: dark) {{
        :root {{
            --bg-color: #222;
            --text-color: #ddd;
        }}
    }}
    </style>
</head>
<body>
    <div>
        <h1>{0}: {1}</h1>{2}{3}<hr />
        <footer><a href="https://salvo.rs" target="_blank">salvo</a></footer>
    </div>
</body>
</html>"#,
        code.as_u16(),
        name,
        summary
            .map(|summary| format!("<h3>{}</h3>", summary))
            .unwrap_or_default(),
        detail.map(|detail| format!("<p>{}</p>", detail)).unwrap_or_default(),
    )
}
fn error_json(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        r#"{{"error":{{"code":{},"name":"{}","summary":"{}","detail":"{}"}}}}"#,
        code.as_u16(),
        name,
        summary.unwrap_or(name),
        detail.unwrap_or("there is no more detailed explanation")
    )
}
fn error_text(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        "code:{},\nname:{},\nsummary:{},\ndetail:{}",
        code.as_u16(),
        name,
        summary.unwrap_or(name),
        detail.unwrap_or("there is no more detailed explanation")
    )
}
fn error_xml(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        "<error><code>{}</code><name>{}</name><summary>{}</summary><detail>{}</detail></error>",
        code.as_u16(),
        name,
        summary.unwrap_or(name),
        detail.unwrap_or("there is no more detailed explanation")
    )
}

pub type HttpResult<T> = Result<T, HttpError>;

#[derive(Debug)]
pub struct HttpError {
    pub code: StatusCode,
    pub name: String,
    pub summary: Option<String>,
    pub detail: Option<String>,
}
impl HttpError {
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl StdError for HttpError {}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "name: {}", &self.name)?;
        write!(f, "summary: {:?}", &self.summary)?;
        write!(f, "detail: {:?}", &self.detail)?;
        Ok(())
    }
}
impl HttpError {
    pub fn as_bytes(&self, prefer_format: &Mime) -> (Mime, Vec<u8>) {
        let format = if !SUPPORTED_FORMATS.contains(&prefer_format.subtype()) {
            "text/html".parse().unwrap()
        } else {
            prefer_format.clone()
        };
        let content = match format.subtype().as_ref() {
            "text" => error_text(self.code, &self.name, self.summary.as_deref(), self.detail.as_deref()),
            "json" => error_json(self.code, &self.name, self.summary.as_deref(), self.detail.as_deref()),
            "xml" => error_xml(self.code, &self.name, self.summary.as_deref(), self.detail.as_deref()),
            _ => error_html(self.code, &self.name, self.summary.as_deref(), self.detail.as_deref()),
        };
        (format, content.as_bytes().to_owned())
    }
}
#[async_trait]
impl Writer for HttpError {
    async fn write(mut self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_status_code(self.code);
        let format = crate::http::guess_accept_mime(req, None);
        let (format, data) = self.as_bytes(&format);
        res.render_binary(format.to_string().parse().unwrap(), &data);
    }
}

macro_rules! default_errors {
    ($($sname:ident, $code:expr, $name:expr, $summary:expr);+) => {
        $(
            #[allow(non_snake_case)]
            pub fn $sname() -> HttpError {
                HttpError {
                    code: $code,
                    name: $name.into(),
                    summary: None,
                    detail: None,
                }
            }
        )+
    }
}

pub fn from_code(code: StatusCode) -> Option<HttpError> {
    match code {
        StatusCode::BAD_REQUEST => Some(BadRequest()),
        StatusCode::UNAUTHORIZED => Some(Unauthorized()),
        StatusCode::PAYMENT_REQUIRED => Some(PaymentRequired()),
        StatusCode::FORBIDDEN => Some(Forbidden()),
        StatusCode::NOT_FOUND => Some(NotFound()),
        StatusCode::METHOD_NOT_ALLOWED => Some(MethodNotAllowed()),
        StatusCode::NOT_ACCEPTABLE => Some(NotAcceptable()),
        StatusCode::PROXY_AUTHENTICATION_REQUIRED => Some(ProxyAuthenticationRequired()),
        StatusCode::REQUEST_TIMEOUT => Some(RequestTimeout()),
        StatusCode::CONFLICT => Some(Conflict()),
        StatusCode::GONE => Some(Gone()),
        StatusCode::LENGTH_REQUIRED => Some(LengthRequired()),
        StatusCode::PRECONDITION_FAILED => Some(PreconditionFailed()),
        StatusCode::PAYLOAD_TOO_LARGE => Some(PayloadTooLarge()),
        StatusCode::URI_TOO_LONG => Some(UriTooLong()),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => Some(UnsupportedMediaType()),
        StatusCode::RANGE_NOT_SATISFIABLE => Some(RangeNotSatisfiable()),
        StatusCode::EXPECTATION_FAILED => Some(ExpectationFailed()),
        StatusCode::IM_A_TEAPOT => Some(ImATeapot()),
        StatusCode::MISDIRECTED_REQUEST => Some(MisdirectedRequest()),
        StatusCode::UNPROCESSABLE_ENTITY => Some(UnprocessableEntity()),
        StatusCode::LOCKED => Some(Locked()),
        StatusCode::FAILED_DEPENDENCY => Some(FailedDependency()),
        StatusCode::UPGRADE_REQUIRED => Some(UpgradeRequired()),
        StatusCode::PRECONDITION_REQUIRED => Some(PreconditionRequired()),
        StatusCode::TOO_MANY_REQUESTS => Some(TooManyRequests()),
        StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE => Some(RequestHeaderFieldsTooLarge()),
        StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => Some(UnavailableForLegalReasons()),
        StatusCode::INTERNAL_SERVER_ERROR => Some(InternalServerError()),
        StatusCode::NOT_IMPLEMENTED => Some(NotImplemented()),
        StatusCode::BAD_GATEWAY => Some(BadGateway()),
        StatusCode::SERVICE_UNAVAILABLE => Some(ServiceUnavailable()),
        StatusCode::GATEWAY_TIMEOUT => Some(GatewayTimeout()),
        StatusCode::HTTP_VERSION_NOT_SUPPORTED => Some(HttpVersionNotSupported()),
        StatusCode::VARIANT_ALSO_NEGOTIATES => Some(VariantAlsoNegotiates()),
        StatusCode::INSUFFICIENT_STORAGE => Some(InsufficientStorage()),
        StatusCode::LOOP_DETECTED => Some(LoopDetected()),
        StatusCode::NOT_EXTENDED => Some(NotExtended()),
        StatusCode::NETWORK_AUTHENTICATION_REQUIRED => Some(NetworkAuthenticationRequired()),
        _ => None,
    }
}
default_errors! {
    BadRequest,            StatusCode::BAD_REQUEST,            "Bad Request", "The request could not be understood by the server due to malformed syntax.";
    Unauthorized,          StatusCode::UNAUTHORIZED,           "Unauthorized", "The request requires user authentication.";
    PaymentRequired,       StatusCode::PAYMENT_REQUIRED,       "Payment Required", "The request could not be processed due to lack of payment.";
    Forbidden,             StatusCode::FORBIDDEN,              "Forbidden", "The server refused to authorize the request.";
    NotFound,              StatusCode::NOT_FOUND,              "Not Found", "The requested resource could not be found.";
    MethodNotAllowed,      StatusCode::METHOD_NOT_ALLOWED,     "Method Not Allowed", "The request method is not supported for the requested resource.";
    NotAcceptable,         StatusCode::NOT_ACCEPTABLE,         "Not Acceptable", "The requested resource is capable of generating only content not acceptable according to the Accept headers sent in the request.";
    ProxyAuthenticationRequired, StatusCode::PROXY_AUTHENTICATION_REQUIRED,  "Proxy Authentication Required", "Authentication with the proxy is required.";
    RequestTimeout,        StatusCode::REQUEST_TIMEOUT,        "Request Timeout", "The server timed out waiting for the request.";
    Conflict,              StatusCode::CONFLICT,               "Conflict", "The request could not be processed because of a conflict in the request.";
    Gone,                  StatusCode::GONE,                   "Gone", "The resource requested is no longer available and will not be available again.";
    LengthRequired,        StatusCode::LENGTH_REQUIRED,        "Length Required", "The request did not specify the length of its content, which is required by the requested resource.";
    PreconditionFailed,    StatusCode::PRECONDITION_FAILED,    "Precondition Failed", "The server does not meet one of the preconditions specified in the request.";
    PayloadTooLarge,       StatusCode::PAYLOAD_TOO_LARGE,      "Payload Too Large", "The request is larger than the server is willing or able to process.";
    UriTooLong,            StatusCode::URI_TOO_LONG,           "URI Too Long", "The URI provided was too long for the server to process.";
    UnsupportedMediaType,  StatusCode::UNSUPPORTED_MEDIA_TYPE, "Unsupported Media Type", "The request entity has a media type which the server or resource does not support.";
    RangeNotSatisfiable,   StatusCode::RANGE_NOT_SATISFIABLE,  "Range Not Satisfiable", "The portion of the requested file cannot be supplied by the server.";
    ExpectationFailed,     StatusCode::EXPECTATION_FAILED,     "Expectation Failed", "The server cannot meet the requirements of the expect request-header field.";
    ImATeapot,             StatusCode::IM_A_TEAPOT,            "I'm a teapot", "I was requested to brew coffee, and I am a teapot.";
    MisdirectedRequest,    StatusCode::MISDIRECTED_REQUEST,    "Misdirected Request", "The server cannot produce a response for this request.";
    UnprocessableEntity,   StatusCode::UNPROCESSABLE_ENTITY,   "Unprocessable Entity", "The request was well-formed but was unable to be followed due to semantic errors.";
    Locked,                StatusCode::LOCKED,                 "Locked", "The source or destination resource of a method is locked.";
    FailedDependency,      StatusCode::FAILED_DEPENDENCY,      "Failed Dependency", "The method could not be performed on the resource because the requested action depended on another action and that action failed.";
    UpgradeRequired,       StatusCode::UPGRADE_REQUIRED,       "Upgrade Required", "Switching to the protocol in the Upgrade header field is required.";
    PreconditionRequired,  StatusCode::PRECONDITION_REQUIRED,  "Precondition Required", "The server requires the request to be conditional.";
    TooManyRequests,       StatusCode::TOO_MANY_REQUESTS,      "Too Many Requests", "Too many requests have been received recently.";
    RequestHeaderFieldsTooLarge,   StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,    "Request Header Fields Too Large", "The server is unwilling to process the request because either  an individual header field, or all the header fields collectively, are too large.";
    UnavailableForLegalReasons,    StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,      "Unavailable For Legal Reasons", "The requested resource is unavailable due to a legal demand to deny access to this resource.";
    InternalServerError,        StatusCode::INTERNAL_SERVER_ERROR,  "Internal Server Error", "The server encountered an internal error while processing this request.";
    NotImplemented,        StatusCode::NOT_IMPLEMENTED,        "Not Implemented", "The server either does not recognize the request method, or it lacks the ability to fulfill the request.";
    BadGateway,            StatusCode::BAD_GATEWAY,            "Bad Gateway", "Received an invalid response from an inbound server it accessed while attempting to fulfill the request.";
    ServiceUnavailable,    StatusCode::SERVICE_UNAVAILABLE,    "Service Unavailable", "The server is currently unavailable.";
    GatewayTimeout,        StatusCode::GATEWAY_TIMEOUT,        "Gateway Timeout", "The server did not receive a timely response from an upstream server.";
    HttpVersionNotSupported, StatusCode::HTTP_VERSION_NOT_SUPPORTED, "HTTP Version Not Supported", "The server does not support, or refuses to support, the major version of HTTP that was used in the request message.";
    VariantAlsoNegotiates, StatusCode::VARIANT_ALSO_NEGOTIATES, "Variant Also Negotiates", "The server has an internal configuration error.";
    InsufficientStorage,   StatusCode::INSUFFICIENT_STORAGE,    "Insufficient Storage", "The method could not be performed on the resource because the server is unable to store the representation needed to successfully complete the request.";
    LoopDetected,          StatusCode::LOOP_DETECTED,           "Loop Detected", "the server terminated an operation because it encountered an infinite loop while processing a request with \"Depth: infinity\".";
    NotExtended,           StatusCode::NOT_EXTENDED,            "Not Extended", "Further extensions to the request are required for the server to fulfill it.";
    NetworkAuthenticationRequired, StatusCode::NETWORK_AUTHENTICATION_REQUIRED, "Network Authentication Required", "the client needs to authenticate to gain network access."
}
