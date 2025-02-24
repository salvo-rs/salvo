//! Writer trait and it's implements.

mod json;
mod redirect;
mod seek;
mod text;

use http::header::{AsHeaderName, IntoHeaderName};
use http::{HeaderMap, StatusCode};
pub use json::Json;
pub use redirect::Redirect;
pub use seek::ReadSeeker;
pub use text::Text;

use crate::http::header::{CONTENT_TYPE, HeaderValue};
use crate::{Depot, Request, Response, async_trait};

/// `Writer` is used to write data to [`Response`].
///
/// `Writer` requires the use of [`Depot`] and Request, which allows for greater flexibility.
/// For scenarios that do not require this flexibility, [`Scribe`] can be used, for example [`Text`], [`Json`] are
/// implemented from [`Scribe`].
#[async_trait]
pub trait Writer {
    /// Write data to [`Response`].
    #[must_use = "write future must be used"]
    async fn write(self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}

/// `Scribe` is used to write data to [`Response`].
///
/// `Scribe` is simpler than [`Writer`] and it implements [`Writer`]. It does not require the use of Depot and Request.
///
/// There are several built-in implementations of the `Scribe` trait.
pub trait Scribe {
    /// Render data to [`Response`].
    fn render(self, res: &mut Response);
}
#[async_trait]
impl<P> Writer for P
where
    P: Scribe + Sized + Send,
{
    #[inline]
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        self.render(res)
    }
}

#[async_trait]
impl<P> Writer for Option<P>
where
    P: Scribe + Sized + Send,
{
    #[inline]
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match self {
            Some(v) => v.render(res),
            None => {
                res.status_code(StatusCode::NOT_FOUND);
            }
        }
    }
}

#[async_trait]
impl<T, E> Writer for Result<T, E>
where
    T: Writer + Send,
    E: Writer + Send,
{
    #[inline]
    async fn write(self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
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

#[allow(clippy::unit_arg)]
impl Scribe for () {
    #[inline]
    fn render(self, _res: &mut Response) {}
}

impl Scribe for StatusCode {
    #[inline]
    fn render(self, res: &mut Response) {
        res.status_code(self);
    }
}

impl Scribe for &'static str {
    #[inline]
    fn render(self, res: &mut Response) {
        try_set_header(
            &mut res.headers,
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        let _ = res.write_body(self);
    }
}
impl Scribe for &String {
    #[inline]
    fn render(self, res: &mut Response) {
        try_set_header(
            &mut res.headers,
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        let _ = res.write_body(self.as_bytes().to_vec());
    }
}
impl Scribe for String {
    #[inline]
    fn render(self, res: &mut Response) {
        try_set_header(
            &mut res.headers,
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        let _ = res.write_body(self);
    }
}
impl Scribe for std::convert::Infallible {
    #[inline]
    fn render(self, _res: &mut Response) {}
}

macro_rules! writer_tuple_impls {
    ($(
        $Tuple:tt {
            $(($idx:tt) -> $T:ident,)+
        }
    )+) => {$(
        #[async_trait::async_trait]
        impl<$($T,)+> Writer for ($($T,)+) where $($T: Writer + Send,)+
        {
            async fn write(self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
                $(
                    self.$idx.write(req, depot, res).await;
                )+
            }
        })+
    }
}

crate::for_each_tuple!(writer_tuple_impls);

#[inline(always)]
fn try_set_header<K, V>(headers: &mut HeaderMap<V>, key: K, val: V)
where
    K: IntoHeaderName,
    for<'a> &'a K: AsHeaderName,
{
    if !headers.contains_key(&key) {
        let _ = headers.insert(key, val);
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

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
}
