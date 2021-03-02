use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct And<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for And<T, U>
where
    T: Filter,
    U: Filter,
{
    #[inline]
    fn filter(&self, req: &mut Request, path: &mut PathState) -> bool {
        if !self.first.filter(req, path) {
            false
        } else {
            self.second.filter(req, path)
        }
    }
}
