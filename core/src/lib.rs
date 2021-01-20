mod catcher;
pub mod depot;
mod handler;
pub mod http;
pub mod routing;
pub mod server;
pub mod writer;
mod error;
#[cfg(feature = "tls")]
mod tls;

#[macro_use]
extern crate pin_utils;
#[macro_use]
extern crate futures_util;

pub use self::catcher::{Catcher, CatcherImpl};
pub use self::depot::Depot;
pub use self::handler::Handler;
pub use self::http::{Request, Response};
pub use self::routing::Router;
pub use self::server::Server;
pub use self::writer::Writer;
pub use self::error::Error;
#[cfg(feature = "tls")]
pub use self::server::TlsServer;
pub use salvo_macros::fn_handler;

use std::ops::{Bound, RangeBounds};

trait StringUtils {
    fn substring(&self, start: usize, len: usize) -> &str;
    fn slice(&self, range: impl RangeBounds<usize>) -> &str;
}

impl StringUtils for str {
    fn substring(&self, start: usize, len: usize) -> &str {
        let mut char_pos = 0;
        let mut byte_start = 0;
        let mut it = self.chars();
        loop {
            if char_pos == start {
                break;
            }
            if let Some(c) = it.next() {
                char_pos += 1;
                byte_start += c.len_utf8();
            } else {
                break;
            }
        }
        char_pos = 0;
        let mut byte_end = byte_start;
        loop {
            if char_pos == len {
                break;
            }
            if let Some(c) = it.next() {
                char_pos += 1;
                byte_end += c.len_utf8();
            } else {
                break;
            }
        }
        &self[byte_start..byte_end]
    }
    fn slice(&self, range: impl RangeBounds<usize>) -> &str {
        let start = match range.start_bound() {
            Bound::Included(bound) | Bound::Excluded(bound) => *bound,
            Bound::Unbounded => 0,
        };
        let len = match range.end_bound() {
            Bound::Included(bound) => *bound + 1,
            Bound::Excluded(bound) => *bound,
            Bound::Unbounded => self.len(),
        } - start;
        self.substring(start, len)
    }
}

pub mod prelude {
    pub use crate::depot::Depot;
    pub use crate::http::{Request, Response, StatusCode, HttpError};
    pub use crate::routing::filter;
    pub use crate::routing::Router;
    pub use crate::server::Server;
    pub use crate::writer::*;
    pub use crate::Handler;
    pub use async_trait::async_trait;
    pub use salvo_macros::fn_handler;
}
