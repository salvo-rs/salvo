use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::{ready, TryFuture};

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

#[async_trait]
impl<T, F> Filter for AndThen<T, F>
where
    T: Filter,
    F: Fn() -> Filter,
{
    #[inline]
    async fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
        async move {
            if !self.filter.execute(req, path).await {
                false
            } else {
                self.callback.call().execute(req, path).await
            }
        }
    }
}
