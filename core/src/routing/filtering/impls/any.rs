use std::future::{ready, Future};

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub struct AnyFilter;
impl Filter for AnyFilter {
    type Future = AnyFuture;
    #[inline]
    fn execute(&self, _req: &mut Request, _path: &mut PathState) -> Self::Future {
        AnyFuture
    }
}

impl AnyFilter {
    pub fn new() -> Self {
        AnyFilter
    }
}

#[allow(missing_debug_implementations)]
struct AnyFuture;

impl Future for AnyFuture {
    type Output = bool;

    #[inline]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        Poll::Ready(true))
    }
}
