use std::fmt::{self, Debug, Display, Formatter};

use super::{Scribe, try_set_header};
use crate::http::Response;
use crate::http::header::{CONTENT_TYPE, HeaderValue};

/// Write text content to response as text content.
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello(res: &mut Response) -> Text<&'static str> {
///     Text::Plain("hello")
/// }
/// ```
#[non_exhaustive]
pub enum Text<C> {
    /// It will set `content-type` to `text/plain; charset=utf-8`.
    Plain(C),
    /// It will set `content-type` to `application/json; charset=utf-8`.
    Json(C),
    /// It will set `content-type` to `application/xml; charset=utf-8`.
    Xml(C),
    /// It will set `content-type` to `text/html; charset=utf-8`.
    Html(C),
    /// It will set `content-type` to `text/javascript; charset=utf-8`.
    Js(C),
    /// It will set `content-type` to `text/css; charset=utf-8`.
    Css(C),
    /// It will set `content-type` to `text/csv; charset=utf-8`.
    Csv(C),
    /// It will set `content-type` to `application/atom+xml; charset=utf-8`.
    Atom(C),
    /// It will set `content-type` to `application/rss+xml; charset=utf-8`.
    Rss(C),
    /// It will set `content-type` to `application/rdf+xml; charset=utf-8`.
    Rdf(C),
}

impl<C> Text<C>
where
    C: AsRef<str>,
{
    fn try_set_header(self, res: &mut Response) -> C {
        let (ctype, content) = match self {
            Self::Plain(content) => (
                HeaderValue::from_static("text/plain; charset=utf-8"),
                content,
            ),
            Self::Json(content) => (
                HeaderValue::from_static("application/json; charset=utf-8"),
                content,
            ),
            Self::Xml(content) => (
                HeaderValue::from_static("application/xml; charset=utf-8"),
                content,
            ),
            Self::Html(content) => (
                HeaderValue::from_static("text/html; charset=utf-8"),
                content,
            ),
            Self::Js(content) => (
                HeaderValue::from_static("text/javascript; charset=utf-8"),
                content,
            ),
            Self::Css(content) => (HeaderValue::from_static("text/css; charset=utf-8"), content),
            Self::Csv(content) => (HeaderValue::from_static("text/csv; charset=utf-8"), content),
            Self::Atom(content) => (
                HeaderValue::from_static("application/atom+xml; charset=utf-8"),
                content,
            ),
            Self::Rss(content) => (
                HeaderValue::from_static("application/rss+xml; charset=utf-8"),
                content,
            ),
            Self::Rdf(content) => (
                HeaderValue::from_static("application/rdf+xml; charset=utf-8"),
                content,
            ),
        };
        try_set_header(&mut res.headers, CONTENT_TYPE, ctype);
        content
    }
}
impl Scribe for Text<&'static str> {
    #[inline]
    fn render(self, res: &mut Response) {
        let content = self.try_set_header(res);
        let _ = res.write_body(content);
    }
}
impl Scribe for Text<String> {
    #[inline]
    fn render(self, res: &mut Response) {
        let content = self.try_set_header(res);
        let _ = res.write_body(content);
    }
}
impl Scribe for Text<&String> {
    #[inline]
    fn render(self, res: &mut Response) {
        let content = self.try_set_header(res);
        let _ = res.write_body(content.as_bytes().to_vec());
    }
}
impl<C: Debug> Debug for Text<C> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Text::Plain(content) => f.debug_tuple("Text::Plain").field(content).finish(),
            Text::Json(content) => f.debug_tuple("Text::Json").field(content).finish(),
            Text::Xml(content) => f.debug_tuple("Text::Xml").field(content).finish(),
            Text::Html(content) => f.debug_tuple("Text::Html").field(content).finish(),
            Text::Js(content) => f.debug_tuple("Text::Js").field(content).finish(),
            Text::Css(content) => f.debug_tuple("Text::Css").field(content).finish(),
            Text::Csv(content) => f.debug_tuple("Text::Csv").field(content).finish(),
            Text::Atom(content) => f.debug_tuple("Text::Atom").field(content).finish(),
            Text::Rss(content) => f.debug_tuple("Text::Rss").field(content).finish(),
            Text::Rdf(content) => f.debug_tuple("Text::Rdf").field(content).finish(),
        }
    }
}
impl<C: Display> Display for Text<C> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Text::Plain(content) => Display::fmt(content, f),
            Text::Json(content) => Display::fmt(content, f),
            Text::Xml(content) => Display::fmt(content, f),
            Text::Html(content) => Display::fmt(content, f),
            Text::Js(content) => Display::fmt(content, f),
            Text::Css(content) => Display::fmt(content, f),
            Text::Csv(content) => Display::fmt(content, f),
            Text::Atom(content) => Display::fmt(content, f),
            Text::Rss(content) => Display::fmt(content, f),
            Text::Rdf(content) => Display::fmt(content, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    use super::*;
    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_write_str() {
        #[handler]
        async fn test() -> &'static str {
            "hello"
        }

        let router = Router::new().push(Router::with_path("test").get(test));

        let mut res = TestClient::get("http://127.0.0.1:5800/test")
            .send(router)
            .await;
        assert_eq!(res.take_string().await.unwrap(), "hello");
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_write_string() {
        #[handler]
        async fn test() -> String {
            "hello".to_owned()
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:5800/test")
            .send(router)
            .await;
        assert_eq!(res.take_string().await.unwrap(), "hello");
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_write_plain_text() {
        #[handler]
        async fn test() -> Text<&'static str> {
            Text::Plain("hello")
        }

        let router = Router::new().push(Router::with_path("test").get(test));

        let mut res = TestClient::get("http://127.0.0.1:5800/test")
            .send(router)
            .await;
        assert_eq!(res.take_string().await.unwrap(), "hello");
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_write_json_text() {
        #[handler]
        async fn test() -> Text<&'static str> {
            Text::Json(r#"{"hello": "world"}"#)
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:5800/test")
            .send(router)
            .await;
        assert_eq!(res.take_string().await.unwrap(), r#"{"hello": "world"}"#);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_write_html_text() {
        #[handler]
        async fn test() -> Text<&'static str> {
            Text::Html("<html><body>hello</body></html>")
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:5800/test")
            .send(router)
            .await;
        assert_eq!(
            res.take_string().await.unwrap(),
            "<html><body>hello</body></html>"
        );
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
    }
}
