use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> Filter for AndThen<T, F>
where
    T: Filter,
    F: Fn() -> Filter,
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
