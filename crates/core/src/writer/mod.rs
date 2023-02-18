//! Writer trait and it's implements.

mod json;
mod redirect;
mod text;

pub use json::Json;
pub use redirect::Redirect;
pub use text::Text;

use crate::http::header::{HeaderValue, CONTENT_TYPE};
use crate::{async_trait, Depot, Request, Response};

/// Writer is used to write data to response.
#[async_trait]
pub trait Writer {
    /// Write data to [`Response`].
    #[must_use = "write future must be used"]
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}

#[async_trait]
impl<T, E> Writer for Result<T, E>
where
    T: Writer + Send,
    E: Writer + Send,
{
    #[inline]
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

/// `Piece` is used to write data to [`Response`].
///
/// `Piece` is simpler than [`Writer`] ant it implements [`Writer`].
pub trait Piece {
    /// Render data to [`Response`].
    fn render(self, res: &mut Response);
}
#[async_trait]
impl<P> Writer for P
where
    P: Piece + Sized + Send,
{
    #[inline]
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        self.render(res)
    }
}

#[allow(clippy::unit_arg)]
impl Piece for () {
    #[inline]
    fn render(self, _res: &mut Response) {}
}
impl Piece for &'static str {
    #[inline]
    fn render(self, res: &mut Response) {
        res.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        res.write_body(self).ok();
    }
}
impl<'a> Piece for &'a String {
    #[inline]
    fn render(self, res: &mut Response) {
        res.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        res.write_body(self.as_bytes().to_vec()).ok();
    }
}
impl Piece for String {
    #[inline]
    fn render(self, res: &mut Response) {
        res.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        res.write_body(self).ok();
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_write_str() {
        #[handler(internal)]
        async fn test() -> &'static str {
            "hello"
        }

        let router = Router::new().push(Router::with_path("test").get(test));

        let mut res = TestClient::get("http://127.0.0.1:5800/test").send(router).await;
        assert_eq!(res.take_string().await.unwrap(), "hello");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn test_write_string() {
        #[handler(internal)]
        async fn test() -> String {
            "hello".to_owned()
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:5800/test").send(router).await;
        assert_eq!(res.take_string().await.unwrap(), "hello");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");
    }
}
