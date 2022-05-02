//! Catcher tarit and it's impl.
use mime::Mime;
use once_cell::sync::Lazy;

use crate::http::StatusError;
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
pub struct CatcherImpl;
impl Catcher for CatcherImpl {
    fn catch(&self, req: &Request, _depot: &Depot, res: &mut Response) -> bool {
        let status = res.status_code().unwrap_or(StatusCode::NOT_FOUND);
        if !status.is_server_error() && !status.is_client_error() {
            return false;
        }
        let format = guess_accept_mime(req, None);
        let (format, data) = if res.status_error.is_some() {
            status_error_bytes(res.status_error.as_ref().unwrap(), &format)
        } else {
            status_error_bytes(&StatusError::from_code(status).unwrap(), &format)
        };
        res.headers_mut()
            .insert(header::CONTENT_TYPE, format.to_string().parse().unwrap());
        res.write_body(&data).ok();
        true
    }
}
