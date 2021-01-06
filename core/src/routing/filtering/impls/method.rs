use std::future::{ready, Ready};

use async_trait::async_trait;

use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

pub struct MethodFilter(Method);

#[async_trait]
impl Filter for MethodFilter {
    #[inline]
    async fn execute(&self, req: &mut Request, _path: PathState) -> bool {
        req.method() == self.0
    }
}

pub fn get() -> MethodFilter {
    MethodFilter(Method::GET)
}
pub fn head() -> MethodFilter {
    MethodFilter(Method::HEAD)
}
pub fn options() -> MethodFilter {
    MethodFilter(Method::OPTIONS)
}
pub fn post() -> MethodFilter {
    MethodFilter(Method::POST)
}
pub fn patch() -> MethodFilter {
    MethodFilter(Method::PATCH)
}
pub fn put() -> MethodFilter {
    MethodFilter(Method::PUT)
}
pub fn delete() -> MethodFilter {
    MethodFilter(Method::DELETE)
}
