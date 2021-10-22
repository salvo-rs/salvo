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
pub use impls::*;

use crate::http::Method;

pub trait Filter: Send + Sync + 'static {
    fn and<F>(self, other: F) -> And<Self, F>
    where
        Self: Sized,
        F: Filter + Sync + Send,
    {
        And {
            first: self,
            second: other,
        }
    }

    fn or<F>(self, other: F) -> Or<Self, F>
    where
        Self: Sized,
        F: Filter + Sync + Send,
    {
        Or {
            first: self,
            second: other,
        }
    }

    fn and_then<F, U>(self, fun: F) -> AndThen<Self, F>
    where
        Self: Sized,
        F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
        U: Filter + Sync + Send,
    {
        AndThen {
            filter: self,
            callback: fun,
        }
    }

    fn or_else<F, U>(self, fun: F) -> OrElse<Self, F>
    where
        Self: Sized,
        F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
        U: Filter + Sync + Send,
    {
        OrElse {
            filter: self,
            callback: fun,
        }
    }

    fn filter(&self, req: &mut Request, path: &mut PathState) -> bool;
}

// ===== FnFilter =====
#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub struct FnFilter<F>(pub F);

impl<F> Filter for FnFilter<F>
where
    F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
{
    #[inline]
    fn filter(&self, req: &mut Request, path: &mut PathState) -> bool {
        self.0(req, path)
    }
}

#[inline]
pub fn path(path: impl Into<String>) -> PathFilter {
    PathFilter::new(path)
}
#[inline]
pub fn get() -> MethodFilter {
    MethodFilter(Method::GET)
}
#[inline]
pub fn head() -> MethodFilter {
    MethodFilter(Method::HEAD)
}
#[inline]
pub fn options() -> MethodFilter {
    MethodFilter(Method::OPTIONS)
}
#[inline]
pub fn post() -> MethodFilter {
    MethodFilter(Method::POST)
}
#[inline]
pub fn patch() -> MethodFilter {
    MethodFilter(Method::PATCH)
}
#[inline]
pub fn put() -> MethodFilter {
    MethodFilter(Method::PUT)
}
#[inline]
pub fn delete() -> MethodFilter {
    MethodFilter(Method::DELETE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_methods() {
       assert!(get() == MethodFilter(Method::GET));
       assert!(head() == MethodFilter(Method::HEAD));
       assert!(options() == MethodFilter(Method::OPTIONS));
       assert!(post() == MethodFilter(Method::POST));
       assert!(patch() == MethodFilter(Method::PATCH));
       assert!(put() == MethodFilter(Method::PUT));
       assert!(delete() == MethodFilter(Method::DELETE));
    }
}
