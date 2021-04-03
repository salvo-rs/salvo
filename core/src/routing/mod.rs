pub mod filter;
mod router;
pub use filter::*;
pub use router::{DetectMatched, Router};

use std::collections::HashMap;
pub type PathParams = HashMap<String, String>;

#[derive(Debug, Eq, PartialEq)]
pub struct PathState {
    pub(crate) url_path: String,
    pub(crate) cursor: usize,
    pub(crate) params: PathParams,
}
impl PathState {
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
