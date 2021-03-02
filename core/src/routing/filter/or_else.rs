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
    F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
{
    #[inline]
    fn filter(&self, req: &mut Request, path: &mut PathState) -> bool {
        if self.filter.filter(req, path) {
            true
        } else {
            (self.callback)(req, path)
        }
    }
}
