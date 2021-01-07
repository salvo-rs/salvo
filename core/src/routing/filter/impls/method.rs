use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

pub struct MethodFilter(pub Method);

impl Filter for MethodFilter {
    #[inline]
    fn execute(&self, req: &mut Request, _path: &mut PathState) -> bool {
        req.method() == self.0
    }
}
