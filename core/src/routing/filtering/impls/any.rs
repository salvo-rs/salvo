use std::future::{ready, Future};

use async_trait::async_trait;

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub struct AnyFilter;
#[async_trait]
impl Filter for AnyFilter {
    #[inline]
    async fn execute(&self, _req: &mut Request, _path: &mut PathState) -> bool {
        true
    }
}
