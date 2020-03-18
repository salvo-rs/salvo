use async_trait::async_trait;

use crate::http::{Request, Response};

mod named_file;
pub use named_file::NamedFile;

#[async_trait]
pub trait Content: Send {
    async fn apply(mut self, req: &mut Request, resp: &mut Response);
}

pub struct HtmlTextContent<T>(T);
#[async_trait]
impl<T> Content for HtmlTextContent<T> where T: AsRef<str> + Send {
    async fn apply(mut self, _req: &mut Request, resp: &mut Response) {
        resp.render("text/html", self.0.as_ref().as_bytes());
    }
}

pub struct JsonTextContent<T>(T);
#[async_trait]
impl<T> Content for JsonTextContent<T> where T: AsRef<str> + Send {
    async fn apply(mut self, _req: &mut Request, resp: &mut Response) {
        resp.render("application/json", self.0.as_ref().as_bytes());
    }
}

pub struct PlainTextContent<T>(T);
#[async_trait]
impl<T> Content for PlainTextContent<T> where T: AsRef<str> + Send {
    async fn apply(mut self, _req: &mut Request, resp: &mut Response) {
        resp.render("text/plain", self.0.as_ref().as_bytes());
    }
}

pub struct XmlTextContent<T>(T);
#[async_trait]
impl<T> Content for XmlTextContent<T> where T: AsRef<str> + Send {
    async fn apply(mut self, _req: &mut Request, resp: &mut Response) {
        resp.render("text/xml", self.0.as_ref().as_bytes());
    }
}

#[async_trait]
impl Content for () {
    async fn apply(mut self, _req: &mut Request, _resp: &mut Response) {
    }
}