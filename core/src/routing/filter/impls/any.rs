use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub struct AnyFilter;
impl Filter for AnyFilter {
    #[inline]
    fn execute(&self, _req: &mut Request, _path: &mut PathState) -> bool {
        true
    }
}
