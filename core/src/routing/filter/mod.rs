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

    fn and_then<F>(self, fun: F) -> AndThen<Self, F>
    where
        Self: Sized,
        F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
    {
        AndThen {
            filter: self,
            callback: fun,
        }
    }

    fn or_else<F>(self, fun: F) -> OrElse<Self, F>
    where
        Self: Sized,
        F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
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

    #[test]
    fn test_opts() {
        fn has_one(_req: &mut Request, path: &mut PathState) -> bool {
            path.url_path.contains("one")
        }
        fn has_two(_req: &mut Request, path: &mut PathState) -> bool {
            path.url_path.contains("two")
        }

        let one_filter = FnFilter(has_one);
        let two_filter = FnFilter(has_two);

        let mut request = Request::default();
        let mut path_state = PathState::new("http://localhost/one");
        assert!(one_filter.filter(&mut request, &mut path_state));
        assert!(!two_filter.filter(&mut request, &mut path_state));
        assert!(one_filter.or_else(has_two).filter(&mut request, &mut path_state));
        assert!(one_filter.or(two_filter).filter(&mut request, &mut path_state));
        assert!(!one_filter.and_then(has_two).filter(&mut request, &mut path_state));
        assert!(!one_filter.and(two_filter).filter(&mut request, &mut path_state));

        let mut path_state = PathState::new("http://localhost/one/two");
        assert!(one_filter.filter(&mut request, &mut path_state));
        assert!(two_filter.filter(&mut request, &mut path_state));
        assert!(one_filter.or_else(has_two).filter(&mut request, &mut path_state));
        assert!(one_filter.or(two_filter).filter(&mut request, &mut path_state));
        assert!(one_filter.and_then(has_two).filter(&mut request, &mut path_state));
        assert!(one_filter.and(two_filter).filter(&mut request, &mut path_state));

        let mut path_state = PathState::new("http://localhost/two");
        assert!(!one_filter.filter(&mut request, &mut path_state));
        assert!(two_filter.filter(&mut request, &mut path_state));
        assert!(one_filter.or_else(has_two).filter(&mut request, &mut path_state));
        assert!(one_filter.or(two_filter).filter(&mut request, &mut path_state));
        assert!(!one_filter.and_then(has_two).filter(&mut request, &mut path_state));
        assert!(!one_filter.and(two_filter).filter(&mut request, &mut path_state));
    }
}
