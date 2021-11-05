use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::http::errors::*;
use crate::http::header::HeaderValue;
use crate::http::{Request, Response};
use crate::Depot;

/// Writer is used to write data to response.
#[async_trait]
pub trait Writer {
    /// Write data to ```Respone```.
    #[must_use = "write future must be used"]
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}

#[allow(clippy::unit_arg)]
#[async_trait]
impl Writer for () {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {}
}
#[async_trait]
impl<T, E> Writer for Result<T, E>
where
    T: Writer + Send,
    E: Writer + Send,
{
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            Ok(v) => {
                v.write(req, depot, res).await;
            }
            Err(e) => {
                e.write(req, depot, res).await;
            }
        }
    }
}
#[async_trait]
impl<'a> Writer for &'a str {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render_binary(HeaderValue::from_static("text/plain; charset=utf-8"), self.as_bytes());
    }
}
#[async_trait]
impl<'a> Writer for &'a String {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        (&**self).write(_req, _depot, res).await;
    }
}
#[async_trait]
impl Writer for String {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        (&*self).write(_req, _depot, res).await;
    }
}

/// Write text content to response as text content. It will set ```content-type``` to ```text/plain; charset=utf-8```.
pub struct PlainText<T>(T);
#[async_trait]
impl<T> Writer for PlainText<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render_binary(
            HeaderValue::from_static("text/plain; charset=utf-8"),
            self.0.as_ref().as_bytes(),
        );
    }
}

/// Write text content to response as json content. It will set ```content-type``` to ```application/json; charset=utf-8```.
pub struct JsonText<T>(T);
#[async_trait]
impl<T> Writer for JsonText<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match serde_json::from_str::<Value>(self.0.as_ref()) {
            Ok(_) => {
                res.render_binary(
                    HeaderValue::from_static("application/json; charset=utf-8"),
                    self.0.as_ref().as_bytes(),
                );
            }
            Err(e) => {
                tracing::error!(error = ?e, "JsonText write error");
                res.set_http_error(InternalServerError());
            }
        }
    }
}

/// Write text content to response as html content. It will set ```content-type``` to ```text/html; charset=utf-8```.
pub struct HtmlText<T>(T);
#[async_trait]
impl<T> Writer for HtmlText<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render_binary(
            HeaderValue::from_static("text/html; charset=utf-8"),
            self.0.as_ref().as_bytes(),
        );
    }
}

/// Write serializable content to response as json content. It will set ```content-type``` to ```application/json; charset=utf-8```.
pub struct JsonContent<T>(T);
#[async_trait]
impl<T> Writer for JsonContent<T>
where
    T: Serialize + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match serde_json::to_vec(&self.0) {
            Ok(bytes) => {
                res.render_binary(HeaderValue::from_static("application/json; charset=utf-8"), &bytes);
            }
            Err(e) => {
                tracing::error!(error = ?e, "JsonContent write error");
                res.set_http_error(InternalServerError());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    use super::*;

    async fn access(service: &Service) -> Response {
        let req: Request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/test")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        service.handle(req).await
    }

    #[tokio::test]
    async fn test_write_str() {
        #[fn_handler]
        async fn test() -> &'static str {
            "hello"
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let mut res = access(&service).await;
        assert_eq!(res.take_text().await.unwrap(), "hello");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn test_write_string() {
        #[fn_handler]
        async fn test() -> String {
            "hello".to_owned()
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let mut res = access(&service).await;
        assert_eq!(res.take_text().await.unwrap(), "hello");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn test_write_plain_text() {
        #[fn_handler]
        async fn test() -> PlainText<&'static str> {
            PlainText("hello")
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let mut res = access(&service).await;
        assert_eq!(res.take_text().await.unwrap(), "hello");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn test_write_json_text() {
        #[fn_handler]
        async fn test() -> JsonText<&'static str> {
            JsonText(r#"{"hello": "world"}"#)
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let mut res = access(&service).await;
        assert_eq!(res.take_text().await.unwrap(), r#"{"hello": "world"}"#);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_write_json_text_error() {
        #[fn_handler]
        async fn test() -> JsonText<&'static str> {
            JsonText(r#"{"hello": "world}"#)
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let response = access(&service).await;
        assert_eq!(response.status_code().unwrap(), 500);
    }

    #[tokio::test]
    async fn test_write_json_content() {
        #[derive(Serialize, Debug)]
        struct User {
            name: String,
        }
        #[fn_handler]
        async fn test() -> JsonContent<User> {
            JsonContent(User { name: "jobs".into() })
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let mut res = access(&service).await;
        assert_eq!(res.take_text().await.unwrap(), r#"{"name":"jobs"}"#);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_write_html_text() {
        #[fn_handler]
        async fn test() -> HtmlText<&'static str> {
            HtmlText("<html><body>hello</body></html>")
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let service = Service::new(router);

        let mut res = access(&service).await;
        assert_eq!(res.take_text().await.unwrap(), "<html><body>hello</body></html>");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/html; charset=utf-8");
    }
}
