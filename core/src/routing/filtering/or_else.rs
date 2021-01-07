use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct OrElse<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> Filter for OrElse<T, F>
where
    T: Filter,
    F: Fn() -> Box<dyn Filter>,
{
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
        if self.filter.execute(req, path) {
            true
        } else {
            self.callback.call().execute(req, path)
        }
    }
}
