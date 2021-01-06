use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::{ready, TryFuture};

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

#[async_trait]
impl<T, U> Filter for Or<T, U>
where
    T: Filter + Send,
    U: Filter + Send,
{
    async fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
        async move {
            if self.first.execute(req, path).await {
                true
            } else {
                self.second.execute(req, path).await
            }
        }
        .await
    }
}
