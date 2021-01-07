mod and;
mod and_then;
pub(crate) mod impls;
mod or;
mod or_else;

pub(crate) use self::and::And;
use self::and_then::AndThen;
pub(crate) use self::or::Or;
use self::or_else::OrElse;
use crate::http::Request;
use crate::routing::PathState;
pub(crate) use impls::*;

use crate::http::Method;

pub trait Filter: Send + 'static {
    fn and<F>(self, other: F) -> And<Self, F>
    where
        Self: Sized,
        F: Filter + Clone,
    {
        And { first: self, second: other }
    }

    fn or<F>(self, other: F) -> Or<Self, F>
    where
        Self: Filter + Sized,
        F: Filter,
    {
        Or { first: self, second: other }
    }

    fn and_then<F, U>(self, fun: F) -> AndThen<Self, F>
    where
        Self: Sized,
        F: Fn() -> U,
        U: Filter + Send,
    {
        AndThen { filter: self, callback: fun }
    }

    fn or_else<F, U>(self, fun: F) -> OrElse<Self, F>
    where
        Self: Filter,
        F: Fn() -> U,
        U: Filter + Send,
    {
        OrElse { filter: self, callback: fun }
    }

    fn execute(&self, req: &mut Request, path: &mut PathState) -> bool;
}

// ===== FilterFn =====

pub(crate) fn filter_fn<F>(func: F) -> FilterFn<F>
where
    F: Fn(&mut Request, &mut PathState) -> bool,
{
    FilterFn { func }
}

#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub(crate) struct FilterFn<F> {
    func: F,
}

impl<F> Filter for FilterFn<F>
where
    F: Fn(&mut Request, &mut PathState) -> bool,
{
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
        self.func(req, path)
    }
}

pub fn any() -> AnyFilter {
    AnyFilter
}
pub fn path(path: impl Into<String>) -> PathFilter {
    PathFilter::new(path)
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
