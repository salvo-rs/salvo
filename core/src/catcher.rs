//! Catcher tarit and it's impl.
use mime::Mime;
use once_cell::sync::Lazy;

use crate::http::errors::StatusError;
use crate::http::{guess_accept_mime, header, Request, Response, StatusCode};
use crate::Depot;

static SUPPORTED_FORMATS: Lazy<Vec<mime::Name>> = Lazy::new(|| vec![mime::JSON, mime::HTML, mime::XML, mime::PLAIN]);
const EMPTY_DETAIL_MSG: &str = "there is no more detailed explanation";

/// Catch error in current response.
pub trait Catcher: Send + Sync + 'static {
    /// If the current catcher caught the error, it will returns true.
    fn catch(&self, req: &Request, depot: &Depot, res: &mut Response) -> bool;
}
fn status_error_html(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
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
    footer{{text-align:center;}}
    @media (prefers-color-scheme: dark) {{
        :root {{
            --bg-color: #222;
            --text-color: #ddd;
        }}
        a:link {{ color: red; }}
        a:visited {{ color: #a8aeff; }}
        a:hover {{color: #a8aeff;}}
        a:active {{color: #a8aeff;}}
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
        format_args!("<p>{}</p>", detail.unwrap_or(EMPTY_DETAIL_MSG)),
    )
}
fn status_error_json(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        r#"{{"error":{{"code":{},"name":"{}","summary":"{}","detail":"{}"}}}}"#,
        code.as_u16(),
        name,
        summary.unwrap_or(name),
        detail.unwrap_or(EMPTY_DETAIL_MSG)
    )
}
fn status_error_plain(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        "code:{},\nname:{},\nsummary:{},\ndetail:{}",
        code.as_u16(),
        name,
        summary.unwrap_or(name),
        detail.unwrap_or(EMPTY_DETAIL_MSG)
    )
}
fn status_error_xml(code: StatusCode, name: &str, summary: Option<&str>, detail: Option<&str>) -> String {
    format!(
        "<error><code>{}</code><name>{}</name><summary>{}</summary><detail>{}</detail></error>",
        code.as_u16(),
        name,
        summary.unwrap_or(name),
        detail.unwrap_or(EMPTY_DETAIL_MSG)
    )
}
/// Create bytes from `StatusError`.
pub fn status_error_bytes(err: &StatusError, prefer_format: &Mime) -> (Mime, Vec<u8>) {
    let format = if !SUPPORTED_FORMATS.contains(&prefer_format.subtype()) {
        "text/html".parse().unwrap()
    } else {
        prefer_format.clone()
    };
    let content = match format.subtype().as_ref() {
        "plain" => status_error_plain(err.code, &err.name, err.summary.as_deref(), err.detail.as_deref()),
        "json" => status_error_json(err.code, &err.name, err.summary.as_deref(), err.detail.as_deref()),
        "xml" => status_error_xml(err.code, &err.name, err.summary.as_deref(), err.detail.as_deref()),
        _ => status_error_html(err.code, &err.name, err.summary.as_deref(), err.detail.as_deref()),
    };
    (format, content.as_bytes().to_owned())
}
/// Default implementation of Catcher.
pub struct CatcherImpl(StatusCode);
impl CatcherImpl {
    /// Create new `CatcherImpl`.
    pub fn new(code: StatusCode) -> CatcherImpl {
        CatcherImpl(code)
    }
}
impl Catcher for CatcherImpl {
    fn catch(&self, req: &Request, _depot: &Depot, res: &mut Response) -> bool {
        let status = res.status_code().unwrap_or(StatusCode::NOT_FOUND);
        if status != self.0 {
            return false;
        }
        let format = guess_accept_mime(req, None);
        let (format, data) = if res.status_error.is_some() {
            status_error_bytes(res.status_error.as_ref().unwrap(), &format)
        } else {
            status_error_bytes(&StatusError::from_code(self.0).unwrap(), &format)
        };
        res.headers_mut()
            .insert(header::CONTENT_TYPE, format.to_string().parse().unwrap());
        res.write_body(&data);
        true
    }
}

macro_rules! default_catchers {
    ($($code:expr),+) => (
        let list: Vec<Box<dyn Catcher>> = vec![
        $(
            Box::new(CatcherImpl::new($code)),
        )+];
        list
    )
}

/// Defaut catchers.
pub mod defaults {
    use super::{Catcher, CatcherImpl};
    use http::status::StatusCode;

    /// Get a new default catchers list.
    pub fn get() -> Vec<Box<dyn Catcher>> {
        default_catchers! {
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYMENT_REQUIRED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::METHOD_NOT_ALLOWED,
            StatusCode::NOT_ACCEPTABLE,
            StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::CONFLICT,
            StatusCode::GONE,
            StatusCode::LENGTH_REQUIRED,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::PAYLOAD_TOO_LARGE,
            StatusCode::URI_TOO_LONG,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            StatusCode::RANGE_NOT_SATISFIABLE,
            StatusCode::EXPECTATION_FAILED,
            StatusCode::IM_A_TEAPOT,
            StatusCode::MISDIRECTED_REQUEST,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::LOCKED,
            StatusCode::FAILED_DEPENDENCY,
            StatusCode::UPGRADE_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::NOT_IMPLEMENTED,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            StatusCode::VARIANT_ALSO_NEGOTIATES,
            StatusCode::INSUFFICIENT_STORAGE,
            StatusCode::LOOP_DETECTED,
            StatusCode::NOT_EXTENDED,
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED
        }
    }
}
