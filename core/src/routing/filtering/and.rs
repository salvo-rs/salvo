use std::future::{ready, Future};
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::ready;

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct And<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

#[async_trait]
impl<T, U> Filter for And<T, U>
where
    T: Filter + Send,
    U: Filter + Send,
{
    #[inline]
    async fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
        if !self.first.execute(req, path).await {
            false
        } else {
            self.second.execute(req, path).await
        }
    }
}
