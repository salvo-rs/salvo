
use mime::*;
use hyper::Response;
use hyper::body::{Bytes, Sender};
use bytes::BytesMut;
use async_trait::async_trait;

use crate::http::Writer;
use crate::http::header::CONTENT_TYPE;

pub trait Content: Writer {
    fn content_type(&self) -> Mime;
}

pub struct HtmlTextContent<T>(T);
impl<T> Content for HtmlTextContent<T> where T: AsRef<str> + Sync + Send {
    fn content_type(&self) -> Mime {
        TEXT_HTML
    }
}
#[async_trait]
impl<T> Writer for HtmlTextContent<T> where T: AsRef<str> + Sync + Send {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
        res.headers_mut().insert(CONTENT_TYPE, self.content_type().to_string().parse().unwrap());
        resp.body_mut().send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
    }
}

pub struct JsonTextContent<T>(T);
impl<T> Content for JsonTextContent<T> where T: AsRef<str> + Sync + Send {
    fn content_type(&self) -> Mime {
        APPLICATION_JSON
    }
}
#[async_trait]
impl<T> Writer for JsonTextContent<T> where T: AsRef<str> + Sync + Send {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
        res.headers_mut().insert(CONTENT_TYPE, self.content_type().to_string().parse().unwrap());
        resp.body_mut().send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
    }
}

pub struct PlainTextContent<T>(T);
impl<T> Content for PlainTextContent<T> where T: AsRef<str> + Sync + Send {
    fn content_type(&self) -> Mime {
        TEXT_PLAIN
    }
}
#[async_trait]
impl<T> Writer for PlainTextContent<T> where T: AsRef<str> + Sync + Send {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
        res.headers_mut().insert(CONTENT_TYPE, self.content_type().to_string().parse().unwrap());
        resp.body_mut().send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
    }
}

pub struct XmlTextContent<T>(T);
impl<T> Content for XmlTextContent<T> where T: AsRef<str> + Sync + Send {
    fn content_type(&self) -> Mime {
        TEXT_XML
    }
}
#[async_trait]
impl<T> Writer for XmlTextContent<T> where T: AsRef<str> + Sync + Send {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
        res.headers_mut().insert(CONTENT_TYPE, self.content_type().to_string().parse().unwrap());
        resp.body_mut().send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
    }
}
