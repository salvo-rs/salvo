//! Catch and handle errors.
//!
//! If the status code of [`Response`] is an error, and the body of [`Response`] is empty, then salvo
//! will try to use `Catcher` to catch the error and display a friendly error page.
//!
//! You can return a system default [`Catcher`] through [`Catcher::default()`], and then add it to
//! [`Service`](crate::Service):
//!
//! # Example
//!
//! ```
//! use salvo_core::prelude::*;
//! use salvo_core::catcher::Catcher;
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
//! You can add multiple custom error catching handlers to [`Catcher`] through [`Catcher::hoop`]. The custom error
//! handler can call [`FlowCtrl::skip_rest()`] method to skip next error handlers and return early.

use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use bytes::Bytes;
use mime::Mime;
use serde::Serialize;

use crate::handler::{Handler, WhenHoop};
use crate::http::{Request, ResBody, Response, StatusCode, StatusError, guess_accept_mime, header};
use crate::{Depot, FlowCtrl};

static SUPPORTED_FORMATS: LazyLock<Vec<mime::Name>> =
    LazyLock::new(|| vec![mime::JSON, mime::HTML, mime::XML, mime::PLAIN]);
const EMPTY_CAUSE_MSG: &str = "There is no more detailed explanation.";
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
        Catcher {
            goal: Arc::new(DefaultGoal::new()),
            hoops: vec![],
        }
    }
}
impl Catcher {
    /// Create new `Catcher`.
    pub fn new<H: Handler>(goal: H) -> Self {
        Catcher {
            goal: Arc::new(goal),
            hoops: vec![],
        }
    }

    /// Get current catcher's middlewares reference.
    #[inline]
    pub fn hoops(&self) -> &Vec<Arc<dyn Handler>> {
        &self.hoops
    }
    /// Get current catcher's middlewares mutable reference.
    #[inline]
    pub fn hoops_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.hoops
    }

    /// Add a handler as middleware, it will run the handler when error catched.
    #[inline]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware, it will run the handler when error catched.
    ///
    /// This middleware is only effective when the filter returns true..
    #[inline]
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
#[derive(Default)]
pub struct DefaultGoal {
    footer: Option<Cow<'static, str>>,
}
impl DefaultGoal {
    /// Create new `DefaultGoal`.
    pub fn new() -> Self {
        DefaultGoal { footer: None }
    }
    /// Create new `DefaultGoal` with custom footer.
    #[inline]
    pub fn with_footer(footer: impl Into<Cow<'static, str>>) -> Self {
        Self::new().footer(footer)
    }

    /// Set custom footer which is only used in html error page.
    ///
    /// If footer is `None`, then use default footer.
    /// Default footer is `<a href="https://salvo.rs" target="_blank">salvo</a>`.
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
    <div><h1>{0}: {1}</h1><h3>{2}</h3><pre>{3}</pre><hr><footer>{4}</footer></div>
</body>
</html>"#,
        code.as_u16(),
        name,
        brief,
        cause.unwrap_or(EMPTY_CAUSE_MSG),
        footer.unwrap_or(SALVO_LINK)
    )
}

#[inline]
fn status_error_json(code: StatusCode, name: &str, brief: &str, cause: Option<&str>) -> String {
    #[derive(Serialize)]
    struct Data<'a> {
        error: Error<'a>,
    }
    #[derive(Serialize)]
    struct Error<'a> {
        code: u16,
        name: &'a str,
        brief: &'a str,
        cause: &'a str,
    }
    let data = Data {
        error: Error {
            code: code.as_u16(),
            name,
            brief,
            cause: cause.unwrap_or(EMPTY_CAUSE_MSG),
        },
    };
    serde_json::to_string(&data).unwrap_or_default()
}

fn status_error_plain(code: StatusCode, name: &str, brief: &str, cause: Option<&str>) -> String {
    format!(
        "code: {}\n\nname: {}\n\nbrief: {}\n\ncause: {}",
        code.as_u16(),
        name,
        brief,
        cause.unwrap_or(EMPTY_CAUSE_MSG)
    )
}

fn status_error_xml(code: StatusCode, name: &str, brief: &str, cause: Option<&str>) -> String {
    #[derive(Serialize)]
    struct Data<'a> {
        code: u16,
        name: &'a str,
        brief: &'a str,
        cause: &'a str,
    }

    let data = Data {
        code: code.as_u16(),
        name,
        brief,
        cause: cause.unwrap_or(EMPTY_CAUSE_MSG),
    };
    serde_xml_rs::to_string(&data).unwrap_or_default()
}

/// Create bytes from `StatusError`.
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
    #[cfg(debug_assertions)]
    let cause = err.cause.as_ref().map(|e| format!("{:#?}", e.as_ref()));
    #[cfg(not(debug_assertions))]
    let cause: Option<String> = None;
    let content = match format.subtype().as_ref() {
        "plain" => status_error_plain(err.code, &err.name, &err.brief, cause.as_deref()),
        "json" => status_error_json(err.code, &err.name, &err.brief, cause.as_deref()),
        "xml" => status_error_xml(err.code, &err.name, &err.brief, cause.as_deref()),
        _ => status_error_html(err.code, &err.name, &err.brief, cause.as_deref(), footer),
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
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    use super::*;

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
            TestClient::get(format!("http://127.0.0.1:5800/{}", name))
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
            TestClient::get(format!("http://127.0.0.1:5800/{}", name))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert_eq!(access(&service, "notfound").await, "Custom 404 Error Page");
    }
}
