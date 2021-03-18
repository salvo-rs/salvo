use async_trait::async_trait;
use serde::Serialize;

use crate::http::header::HeaderValue;
use crate::http::errors::*;
use crate::http::{Request, Response};
use crate::Depot;

#[async_trait]
pub trait Writer {
    #[must_use = "write future must be used"]
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}

#[allow(clippy::unit_arg)]
#[async_trait]
impl Writer for () {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {}
}
#[async_trait]
impl<T, E> Writer for Result<T, E> where T: Writer + Send, E: Writer + Send {
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

pub struct PlainText<T>(T);
#[async_trait]
impl<T> Writer for PlainText<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render_binary(HeaderValue::from_static("text/plain; charset=utf-8"), self.0.as_ref().as_bytes());
    }
}

pub struct JsonText<T>(T);
#[async_trait]
impl<T> Writer for JsonText<T>
where
    T: Serialize + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        match serde_json::to_vec(&self.0) {
            Ok(bytes) => {
                res.render_binary(HeaderValue::from_static("application/json; charset=utf-8"), &bytes);
            }
            Err(e) => {
                tracing::error!(error = ?e, "JsonText write error");
                res.set_http_error(InternalServerError());
            }
        }
    }
}

pub struct HtmlText<T>(T);
#[async_trait]
impl<T> Writer for HtmlText<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render_binary(HeaderValue::from_static("text/html; charset=utf-8"), self.0.as_ref().as_bytes());
    }
}