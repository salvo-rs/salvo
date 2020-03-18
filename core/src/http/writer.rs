use bytes::BytesMut;
use hyper::body::{Bytes, Sender};
use hyper::{Response, Body};
use hyper::header::*;
use mime::*;
use async_trait::async_trait;

use crate::http::Request;

#[async_trait]
pub trait Writer: Send {
    async fn write(&mut self, _req: &mut Request, resp: &mut Response<Body>, sender: &mut Sender);
}

#[async_trait]
impl Writer for String {
    async fn write(&mut self, _req: &mut Request, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(self.as_bytes()))).await.ok();
    }
}

#[async_trait]
impl<'a> Writer for &'a str {
    async fn write(&mut self, _req: &mut Request, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(self.as_bytes()))).await.ok();
    }
}

#[async_trait]
impl Writer for Vec<u8> {
    async fn write(&mut self, _req: &mut Request, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(&**self))).await.ok();
    }
}

#[async_trait]
impl<'a> Writer for &'a [u8] {
    async fn write(&mut self, _req: &mut Request, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(*self))).await.ok();
    }
}

#[async_trait]
impl Writer for () {
    async fn write(&mut self, _req: &mut Request, _resp: &mut Response<Body>, _sender: &mut Sender) {
    }
}

// pub struct HtmlText<T>(T);
// #[async_trait]
// impl<T> Writer for HtmlText<T> where T: AsRef<str> + Sync + Send {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         res.headers_mut().insert(CONTENT_TYPE, TEXT_HTML.to_string().parse().unwrap());
//         sender.send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
//     }
// }

// pub struct JsonText<T>(T);
// #[async_trait]
// impl<T> Writer for JsonText<T> where T: AsRef<str> + Sync + Send {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         res.headers_mut().insert(CONTENT_TYPE, APPLICATION_JSON.to_string().parse().unwrap());
//         sender.send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
//     }
// }

// pub struct PlainText<T>(T);
// #[async_trait]
// impl<T> Writer for PlainText<T> where T: AsRef<str> + Sync + Send {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         res.headers_mut().insert(CONTENT_TYPE, TEXT_PLAIN.to_string().parse().unwrap());
//         sender.send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
//     }
// }

// pub struct XmlText<T>(T);
// #[async_trait]
// impl<T> Writer for XmlText<T> where T: AsRef<str> + Sync + Send {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         res.headers_mut().insert(CONTENT_TYPE, TEXT_XML.to_string().parse().unwrap());
//         sender.send_data(Bytes::from(BytesMut::from(self.0.as_ref()))).await.ok();
//     }
// }