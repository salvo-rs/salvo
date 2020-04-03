use async_trait::async_trait;
use std::sync::Arc;

use crate::http::{Request, Response};
use crate::{Depot, ServerConfig};

mod named_file;
pub use named_file::NamedFile;

#[async_trait]
pub trait Writer: Send {
    #[must_use = "future must be used"]
    async fn write(mut self, conf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response);
}

pub struct HtmlTextContent<T>(T);
#[async_trait]
impl<T> Writer for HtmlTextContent<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
        resp.render("text/html", self.0.as_ref().as_bytes());
    }
}

pub struct JsonTextContent<T>(T);
#[async_trait]
impl<T> Writer for JsonTextContent<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
        resp.render("application/json", self.0.as_ref().as_bytes());
    }
}

pub struct PlainTextContent<T>(T);
#[async_trait]
impl<T> Writer for PlainTextContent<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
        resp.render("text/plain", self.0.as_ref().as_bytes());
    }
}

pub struct XmlTextContent<T>(T);
#[async_trait]
impl<T> Writer for XmlTextContent<T>
where
    T: AsRef<str> + Send,
{
    async fn write(mut self, _conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
        resp.render("text/xml", self.0.as_ref().as_bytes());
    }
}

#[async_trait]
impl Writer for () {
    async fn write(mut self, _conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, _resp: &mut Response) {}
}
