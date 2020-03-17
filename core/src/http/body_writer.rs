use std::fs::File;
use std::io::Read;
use hyper::Response;
use hyper::body::{Bytes, Sender};
use bytes::BytesMut;
use async_trait::async_trait;

use super::Body;

/// A trait which writes to an HTTP response.
#[async_trait]
pub trait BodyWriter: Send {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender);
}

#[async_trait]
impl BodyWriter for String {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(&**self))).await.ok();
    }
}

// #[async_trait]
// impl BodyWriter for &'static str {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         sender.send_data(Bytes::from(self));
//     }
// }

#[async_trait]
impl BodyWriter for Vec<u8> {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
        sender.send_data(Bytes::from(BytesMut::from(&**self))).await.ok();
    }
}

// #[async_trait]
// impl BodyWriter for &'static [u8] {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         sender.send_data(Bytes::from(self));
//     }
// }

// #[async_trait]
// impl BodyWriter for File {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         let mut data = Vec::new();
//         self.read_to_end(&mut data).ok();
//         sender.send_data(Bytes::from(data));
//     }
// }

// #[async_trait]
// impl BodyWriter for Box<dyn std::io::Read + Send> {
//     async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
//         let mut data = Vec::new();
//         self.read_to_end(&mut data).ok();
//         sender.send_data(Bytes::from(data));
//     }
// }

#[async_trait]
impl BodyWriter for () {
    async fn write(&mut self, res: &mut Response<Body>, sender: &mut Sender) {
    }
}
