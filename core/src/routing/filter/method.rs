use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

/// Filter by request method
#[derive(Clone, PartialEq, Eq)]
pub struct MethodFilter(pub Method);

impl Filter for MethodFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.method() == self.0
    }
}
