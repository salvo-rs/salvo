pub mod filter;
mod router;
pub use filter::*;
pub use router::{DetectMatched, Router};

use std::collections::HashMap;
pub type Params = HashMap<String, String>;

#[derive(Debug)]
pub struct PathState {
    pub segments: Vec<String>,
    pub match_cursor: usize,
    pub ending_matched: bool,
    pub params: Params,
}
impl PathState {
    pub fn new(segments: Vec<String>) -> Self {
        PathState {
            segments,
            match_cursor: 0,
            ending_matched: false,
            params: Params::new(),
        }
    }
}
