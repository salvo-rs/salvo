//! Routing and filters

pub mod filter;
mod router;
pub use filter::*;
pub use router::{DetectMatched, Router};

use std::collections::HashMap;
use std::sync::Arc;

use crate::http::{Request, Response};
use crate::{Depot, Handler};

#[doc(hidden)]
pub type PathParams = HashMap<String, String>;
#[doc(hidden)]
#[derive(Debug, Eq, PartialEq)]
pub struct PathState {
    pub(crate) url_path: String,
    pub(crate) cursor: usize,
    pub(crate) params: PathParams,
}
impl PathState {
    /// Create new `PathState`.
    pub fn new(url_path: &str) -> Self {
        let url_path = url_path.trim_start_matches('/').trim_end_matches('/');
        PathState {
            url_path: decode_url_path_safely(url_path),
            cursor: 0,
            params: PathParams::new(),
        }
    }
    pub(crate) fn ended(&self) -> bool {
        self.cursor >= self.url_path.len()
    }
}

fn decode_url_path_safely(path: &str) -> String {
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .to_string()
}

/// Flow Control.
pub struct FlowCtrl {
    is_ceased: bool,
    cursor: usize,
    pub(crate) handlers: Vec<Arc<dyn Handler>>,
}

impl FlowCtrl {
    /// Create new `FlowCtrl`.
    #[inline]
    pub fn new(handlers: Vec<Arc<dyn Handler>>) -> Self {
        FlowCtrl {
            is_ceased: false,
            cursor: 0,
            handlers,
        }
    }
    /// Has next handler.
    #[inline]
    pub fn has_next(&self) -> bool {
        self.cursor < self.handlers.len() && !self.handlers.is_empty()
    }

    /// Call next handler. If get next handle and executed, return true, otherwise return false.
    /// If resposne's statuse code is not success or is redirection, all reset handlers will skipped.
    #[inline]
    pub async fn call_next(&mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) -> bool {
        if let Some(code) = res.status_code() {
            if code.is_redirection() || !code.is_success() {
                self.skip_rest();
                return false;
            }
        }
        if let Some(handler) = self.handlers.get(self.cursor) {
            self.cursor += 1;
            handler.clone().handle(req, depot, res, self).await;
            true
        } else {
            false
        }
    }
    /// Skip all reset handlers.
    #[inline]
    pub fn skip_rest(&mut self) {
        self.cursor = self.handlers.len()
    }

    /// Is flow ceased
    #[inline]
    pub fn is_ceased(&self) -> bool {
        self.is_ceased
    }
    /// Cease all fllowwing logic
    #[inline]
    pub fn cease(&mut self) {
        self.skip_rest();
        self.is_ceased = true;
    }
}
