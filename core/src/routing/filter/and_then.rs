use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F, U> Filter for AndThen<T, F>
where
    T: Filter,
    U: Filter,
    F: Fn() -> U,
{
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
        if !self.filter.execute(req, path) {
            false
        } else {
            self.callback.call().execute(req, path)
        }
    }
}
