use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{ready, TryFuture};
use pin_project::pin_project;

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for Or<T, U>
where
    T: Filter,
    U: Filter + Send,
{
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        async move {
            if self.first.execute(req, path).await {
                true
            } else {
                self.second.execute(req, path).await
            }
        }
    }
}
