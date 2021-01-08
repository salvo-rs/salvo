use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for Or<T, U>
where
    T: Filter + Send,
    U: Filter + Send,
{
    #[inline]
    fn filter(&self, req: &mut Request, path: &mut PathState) -> bool {
        if self.first.filter(req, path) {
            true
        } else {
            self.second.filter(req, path)
        }
    }
}
