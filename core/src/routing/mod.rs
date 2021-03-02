pub mod filter;
mod router;
pub use filter::*;
pub use router::{DetectMatched, Router};

use std::collections::HashMap;
pub type Params = HashMap<String, String>;

#[derive(Debug)]
pub struct PathState {
    pub segements: Vec<String>,
    pub match_cursor: usize,
    pub params: Params,
}
impl PathState {
    pub fn new(segements: Vec<String>) -> Self {
        PathState {
            segements,
            match_cursor: 0,
            params: Params::new(),
        }
    }
}
