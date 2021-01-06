use std::future::{ready, Future};

use crate::http::Request;
use crate::routing::{Filter, PathState};

pub struct AnyFilter;
impl Filter for AnyFilter {
    #[inline]
    fn execute(&self, _req: &mut Request, _path: &mut PathState) -> Self::Future {
        ready(true)
    }
}

impl AnyFilter {
    pub fn new() -> Self {
        AnyFilter
    }
}
