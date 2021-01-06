use std::future::{ready, Future};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use pin_project::pin_project;

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
    U: Filter + Clone + Send,
{
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        async move {
            if !self.first.execute(req, path).await {
                false
            } else {
                self.second.execute(req, path).await
            }
        }
    }
}
