mod filtering;
mod router;
pub use filtering::impls::*;
pub use filtering::*;
pub use router::Router;

use std::collections::HashMap;
pub type Params = HashMap<String, String>;

pub struct PathState {
    segements: Vec<String>,
    match_cursor: usize,
    params: Params,
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
