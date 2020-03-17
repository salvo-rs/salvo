use bytes::BytesMut;
use hyper::body::{Bytes, Sender};
use async_trait::async_trait;
use hyper::{Response, Body};

#[async_trait]
pub trait Writer: Send {
    async fn write(&mut self, resp: &mut Response<Body>, sender: &mut Sender);
}

#[async_trait]
impl Writer for String {
    async fn write(&mut self, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(self.as_bytes()))).await.ok();
    }
}

#[async_trait]
impl<'a> Writer for &'a str {
    async fn write(&mut self, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(self.as_bytes()))).await.ok();
    }
}

#[async_trait]
impl Writer for Vec<u8> {
    async fn write(&mut self, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(&**self))).await.ok();
    }
}

#[async_trait]
impl<'a> Writer for &'a [u8] {
    async fn write(&mut self, _resp: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(*self))).await.ok();
    }
}

// #[async_trait]
// impl Writer for File {
//     async fn write(&mut self, resp: &mut Response<Body>, sender: &mut Sender) {
//         std::io::copy(self, resp.body_mut()).map(|_| ());
//     }
// }

// #[async_trait]
// impl Writer for Box<dyn std::io::Read + Send> {
//     async fn write(&mut self, resp: &mut Response<Body>, sender: &mut Sender) {
//         std::io::copy(self, resp.body_mut()).map(|_| ());
//     }
// }

#[async_trait]
impl Writer for () {
    async fn write(&mut self, _resp: &mut Response<Body>, _sender: &mut Sender) {
    }
}
