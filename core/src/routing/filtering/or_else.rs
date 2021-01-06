use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

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
    F: Fn() -> Filter<Future = Future<Output = bool>> + Clone + Send,
{
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        async move {
            if self.filter.execute(req, path).await {
                true
            } else {
                self.callback.call().execute(req, path).await
            }
        }
    }
}
