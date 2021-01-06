use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{ready, TryFuture};
use pin_project::pin_project;

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
    F: Fn() -> bool + Clone + Send,
{
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        async move {
            if !self.filter.execute(req, path).await {
                false
            } else {
                self.callback.call().execute(req, path).await
            }
        }
    }
}
