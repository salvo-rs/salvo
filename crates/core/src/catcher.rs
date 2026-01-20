//! Catch and handle errors.
//!
//! If the status code of [`Response`] is an error, and the body of [`Response`] is empty, then
//! salvo will try to use `Catcher` to catch the error and display a friendly error page.
//!
//! You can return a system default [`Catcher`] through [`Catcher::default()`], and then add it to
//! [`Service`](crate::Service):
//!
//! # Example
//!
//! ```
//! use salvo_core::catcher::Catcher;
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn handle404(&self, res: &mut Response, ctrl: &mut FlowCtrl) {
//!     if let Some(StatusCode::NOT_FOUND) = res.status_code {
//!         res.render("Custom 404 Error Page");
//!         ctrl.skip_rest();
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     Service::new(Router::new()).catcher(Catcher::default().hoop(handle404));
//! }
//! ```
//!
//! The default [`Catcher`] supports sending error pages in `XML`, `JSON`, `HTML`, `Text` formats.
//!
//! You can add a custom error handler to [`Catcher`] by adding `hoop` to the default `Catcher`.
//! The error handler is still [`Handler`].
//!
//! You can add multiple custom error catching handlers to [`Catcher`] through [`Catcher::hoop`].
//! The custom error handler can call [`FlowCtrl::skip_rest()`] method to skip next error handlers
//! and return early.

use std::borrow::Cow;
use std::collections::HashSet;
use std::env;
use std::fmt::{self, Debug, Formatter};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use bytes::Bytes;
use mime::Mime;
use serde::Serialize;

use crate::handler::{Handler, WhenHoop};
use crate::http::mime::guess_accept_mime;
use crate::http::{Request, ResBody, Response, StatusCode, StatusError, header};
use crate::{Depot, FlowCtrl};

static SUPPORTED_FORMATS: LazyLock<Vec<mime::Name>> =
    LazyLock::new(|| vec![mime::JSON, mime::HTML, mime::XML, mime::PLAIN]);
static STATUS_ERROR_SETS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "force_detail",
        "debug_detail",
        "never_detail",
        "force_cause",
        "debug_cause",
        "never_cause",
    ])
});
const SALVO_LINK: &str = r#"<a href="https://salvo.rs" target="_blank">salvo</a>"#;

/// `Catcher` is used to catch errors.
///
/// View [module level documentation](index.html) for more details.
pub struct Catcher {
    goal: Arc<dyn Handler>,
    hoops: Vec<Arc<dyn Handler>>,
}
impl Default for Catcher {
    /// Create new `Catcher` with its goal handler is [`DefaultGoal`].
    fn default() -> Self {
        Self {
            goal: Arc::new(DefaultGoal::new()),
            hoops: vec![],
        }
    }
}
impl Debug for Catcher {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Catcher").finish()
    }
}
impl Catcher {
    /// Create new `Catcher`.
    pub fn new<H: Handler>(goal: H) -> Self {
        Self {
            goal: Arc::new(goal),
            hoops: vec![],
        }
    }

    /// Get current catcher's middlewares reference.
    #[inline]
    #[must_use]
    pub fn hoops(&self) -> &Vec<Arc<dyn Handler>> {
        &self.hoops
    }
    /// Get current catcher's middlewares mutable reference.
    #[inline]
    pub fn hoops_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.hoops
    }

    /// Add a handler as middleware, it will run the handler when error caught.
    #[inline]
    #[must_use]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware, it will run the handler when error caught.
    ///
    /// This middleware is only effective when the filter returns true..
    #[inline]
    #[must_use]
    pub fn hoop_when<H, F>(mut self, hoop: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        self.hoops.push(Arc::new(WhenHoop {
            inner: hoop,
            filter,
        }));
        self
    }

    /// Catch error and send error page.
    pub async fn catch(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        let mut ctrl = FlowCtrl::new(self.hoops.iter().chain([&self.goal]).cloned().collect());
        ctrl.call_next(req, depot, res).await;
    }
}

/// Default [`Handler`] used as goal for [`Catcher`].
///
/// If http status is error, and all custom handlers is not catch it and write body,
/// `DefaultGoal` will used to catch them.
///
/// `DefaultGoal` supports sending error pages in `XML`, `JSON`, `HTML`, `Text` formats.
#[derive(Default, Debug)]
pub struct DefaultGoal {
    footer: Option<Cow<'static, str>>,
}
impl DefaultGoal {
    /// Create new `DefaultGoal`.
    #[must_use]
    pub fn new() -> Self {
        Self { footer: None }
    }
    /// Create new `DefaultGoal` with custom footer.
    #[inline]
    #[must_use]
    pub fn with_footer(footer: impl Into<Cow<'static, str>>) -> Self {
        Self::new().footer(footer)
    }

    /// Set custom footer which is only used in html error page.
    ///
    /// If footer is `None`, then use default footer.
    /// Default footer is `<a href="https://salvo.rs" target="_blank">salvo</a>`.
    #[must_use]
    pub fn footer(mut self, footer: impl Into<Cow<'static, str>>) -> Self {
        self.footer = Some(footer.into());
        self
    }
}
#[async_trait]
impl Handler for DefaultGoal {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        let status = res.status_code.unwrap_or(StatusCode::NOT_FOUND);
        if (status.is_server_error() || status.is_client_error())
            && (res.body.is_none() || res.body.is_error())
        {
            write_error_default(req, res, self.footer.as_deref());
        }
    }
}

fn status_error_html(
    code: StatusCode,
    name: &str,
    brief: &str,
    detail: Option<&str>,
    cause: Option<&str>,
    footer: Option<&str>,
) -> String {
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
    pre {{ text-align: left; padding: 0 1rem; }}
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
    <div><h1>{}: {}</h1><h3>{}</h3>{}{}<hr><footer>{}</footer></div>
</body>
</html>"#,
        code.as_u16(),
        name,
        brief,
        detail
            .map(|detail| format!("<pre>{detail}</pre>"))
            .unwrap_or_default(),
        cause
            .map(|cause| format!("<pre>{cause:#?}</pre>"))
            .unwrap_or_default(),
        footer.unwrap_or(SALVO_LINK)
    )
}

#[inline]
fn status_error_json(
    code: StatusCode,
    name: &str,
    brief: &str,
    detail: Option<&str>,
    cause: Option<&str>,
) -> String {
    #[derive(Serialize)]
    struct Data<'a> {
        error: Error<'a>,
    }
    #[derive(Serialize)]
    struct Error<'a> {
        code: u16,
        name: &'a str,
        brief: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cause: Option<&'a str>,
    }
    let data = Data {
        error: Error {
            code: code.as_u16(),
            name,
            brief,
            detail,
            cause,
        },
    };
    serde_json::to_string(&data).unwrap_or_default()
}

fn status_error_plain(
    code: StatusCode,
    name: &str,
    brief: &str,
    detail: Option<&str>,
    cause: Option<&str>,
) -> String {
    format!(
        "code: {}\n\nname: {}\n\nbrief: {}{}{}",
        code.as_u16(),
        name,
        brief,
        detail
            .map(|detail| format!("\n\ndetail: {detail}"))
            .unwrap_or_default(),
        cause
            .map(|cause| format!("\n\ncause: {cause:#?}"))
            .unwrap_or_default(),
    )
}

fn status_error_xml(
    code: StatusCode,
    name: &str,
    brief: &str,
    detail: Option<&str>,
    cause: Option<&str>,
) -> String {
    #[derive(Serialize)]
    struct Data<'a> {
        code: u16,
        name: &'a str,
        brief: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cause: Option<&'a str>,
    }

    let data = Data {
        code: code.as_u16(),
        name,
        brief,
        detail,
        cause,
    };
    serde_xml_rs::to_string(&data).unwrap_or_default()
}

/// Create bytes from `StatusError`.
///
/// You can use environment variable `SALVO_STATUS_ERROR` to control whether to
/// show `detail` and `cause` information in default error page.
///
/// force_detail: always show detail information in error page even in release mode.
/// debug_detail: only show detail information in error page in debug mode.
/// never_detail: never show detail information in error page.
///
/// force_cause: always show cause information in error page even in release mode.
/// debug_cause: only show cause information in error page in debug mode.
/// never_cause: never show detail information in error page.
///
/// For example:
///
/// ```sh
/// SALVO_STATUS_ERROR=force_cause,force_detail
/// ```
/// will always show `detail` and `cause` information in error page even in release mode.
///
/// If `SALVO_STATUS_ERROR` is not set, then `detail` and `cause` will only be
/// shown in error page in debug mode for security reason.
#[doc(hidden)]
#[inline]
pub fn status_error_bytes(
    err: &StatusError,
    prefer_format: &Mime,
    footer: Option<&str>,
) -> (Mime, Bytes) {
    let format = if !SUPPORTED_FORMATS.contains(&prefer_format.subtype()) {
        mime::TEXT_HTML
    } else {
        prefer_format.clone()
    };

    let env_sets = env::var("SALVO_STATUS_ERROR")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| {
            let s = s.trim().to_lowercase();
            if STATUS_ERROR_SETS.contains(s.as_str()) {
                Some(s)
            } else if s.is_empty() {
                None
            } else {
                tracing::warn!("unknown SALVO_STATUS_ERROR option: {}", s);
                None
            }
        })
        .collect::<HashSet<_>>();

    let detail = if !env_sets.contains("never_detail")
        && (env_sets.contains("force_detail")
            || (env_sets.contains("debug_detail") && cfg!(debug_assertions)))
    {
        err.detail.as_deref()
    } else {
        None
    };

    let cause = if !env_sets.contains("never_cause")
        && (env_sets.contains("force_cause")
            || (env_sets.contains("debug_cause") && cfg!(debug_assertions)))
    {
        err.cause.as_ref().map(|e| format!("{e:#?}"))
    } else {
        None
    };

    let content = match format.subtype().as_ref() {
        "plain" => status_error_plain(err.code, &err.name, &err.brief, detail, cause.as_deref()),
        "json" => status_error_json(err.code, &err.name, &err.brief, detail, cause.as_deref()),
        "xml" => status_error_xml(err.code, &err.name, &err.brief, detail, cause.as_deref()),
        _ => status_error_html(
            err.code,
            &err.name,
            &err.brief,
            detail,
            cause.as_deref(),
            footer,
        ),
    };
    (format, Bytes::from(content))
}

#[doc(hidden)]
pub fn write_error_default(req: &Request, res: &mut Response, footer: Option<&str>) {
    let format = guess_accept_mime(req, None);
    let (format, data) = if let ResBody::Error(body) = &res.body {
        status_error_bytes(body, &format, footer)
    } else {
        let status = res.status_code.unwrap_or(StatusCode::NOT_FOUND);
        status_error_bytes(
            &StatusError::from_code(status).unwrap_or_else(StatusError::internal_server_error),
            &format,
            footer,
        )
    };
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        format.to_string().parse().expect("invalid `Content-Type`"),
    );
    let _ = res.write_body(data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    struct CustomError;
    #[async_trait]
    impl Writer for CustomError {
        async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
            res.status_code = Some(StatusCode::INTERNAL_SERVER_ERROR);
            res.render("custom error");
        }
    }

    #[handler]
    async fn handle404(
        &self,
        _req: &Request,
        _depot: &Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if res.status_code.is_none() || Some(StatusCode::NOT_FOUND) == res.status_code {
            res.render("Custom 404 Error Page");
            ctrl.skip_rest();
        }
    }

    #[tokio::test]
    async fn test_handle_error() {
        #[handler]
        async fn handle_custom() -> Result<(), CustomError> {
            Err(CustomError)
        }
        let router = Router::new().push(Router::with_path("custom").get(handle_custom));
        let service = Service::new(router);

        async fn access(service: &Service, name: &str) -> String {
            TestClient::get(format!("http://127.0.0.1:8698/{name}"))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert_eq!(access(&service, "custom").await, "custom error");
    }

    #[tokio::test]
    async fn test_custom_catcher() {
        #[handler]
        async fn hello() -> &'static str {
            "Hello World"
        }
        let router = Router::new().get(hello);
        let service = Service::new(router).catcher(Catcher::default().hoop(handle404));

        async fn access(service: &Service, name: &str) -> String {
            TestClient::get(format!("http://127.0.0.1:8698/{name}"))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert_eq!(access(&service, "notfound").await, "Custom 404 Error Page");
    }
}
