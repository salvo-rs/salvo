pub mod server;
mod handler;
mod content;
mod catcher;
mod pick_port;
pub mod error;
pub mod http;
pub mod routing;
pub mod depot;
pub mod logging;

// #[macro_use]
extern crate serde;
// #[macro_use]
// extern crate serde_derive;
// #[macro_use]
// extern crate serde_json;
// #[macro_use]
// extern crate mime;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate pin_utils;
#[macro_use]
extern crate futures_util;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;

pub use crate::content::Content;
pub use crate::http::{Request, Response};
pub use crate::server::{Server, ServerConfig};
pub use crate::handler::{Handler, HandleError};
pub use crate::routing::Router;
pub use crate::catcher::{Catcher, CatcherImpl};
pub use crate::error::Error;
pub use crate::depot::Depot;

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
            if char_pos == start { break; }
            if let Some(c) = it.next() {
                char_pos += 1;
                byte_start += c.len_utf8();
            }
            else { break; }
        }
        char_pos = 0;
        let mut byte_end = byte_start;
        loop {
            if char_pos == len { break; }
            if let Some(c) = it.next() {
                char_pos += 1;
                byte_end += c.len_utf8();
            }
            else { break; }
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

#[derive(Clone)]
enum _Protocol {
    Http,
    Https,
}

/// Protocol used to serve content.
#[derive(Clone)]
pub struct Protocol(_Protocol);

impl Protocol {
    /// Plaintext HTTP/1
    pub fn http() -> Protocol {
        Protocol(_Protocol::Http)
    }

    /// HTTP/1 over SSL/TLS
    pub fn https() -> Protocol {
        Protocol(_Protocol::Https)
    }

    /// Returns the name used for this protocol in a URI's scheme part.
    pub fn name(&self) -> &str {
        match self.0 {
            _Protocol::Http => "http",
            _Protocol::Https => "https",
        }
    }
}