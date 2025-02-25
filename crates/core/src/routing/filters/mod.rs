//! Filter module
//!
//! This module provides filters for routing requests based on various criteria
//! such as uri scheme, hostname, port, path, and HTTP method.

mod opts;
mod others;
mod path;

use std::fmt::{self, Debug, Formatter};

use self::opts::*;
use crate::async_trait;
use crate::http::uri::Scheme;
use crate::http::{Method, Request};
use crate::routing::PathState;

pub use others::*;
pub use path::*;

/// Trait for filter request.
///
/// View [module level documentation](../index.html) for more details.

#[async_trait]
pub trait Filter: Debug + Send + Sync + 'static {
    #[doc(hidden)]
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
    #[doc(hidden)]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    /// Create a new filter use `And` filter.
    #[inline]
    fn and<F>(self, other: F) -> And<Self, F>
    where
        Self: Sized,
        F: Filter + Send + Sync,
    {
        And {
            first: self,
            second: other,
        }
    }

    /// Create a new filter use `Or` filter.
    #[inline]
    fn or<F>(self, other: F) -> Or<Self, F>
    where
        Self: Sized,
        F: Filter + Send + Sync,
    {
        Or {
            first: self,
            second: other,
        }
    }

    /// Create a new filter use `AndThen` filter.
    #[inline]
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

    /// Create a new filter use `OrElse` filter.
    #[inline]
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

    /// Filter `Request` and returns false or true.
    async fn filter(&self, req: &mut Request, path: &mut PathState) -> bool;
}

/// `FnFilter` accepts a function as its parameter, using this function to filter requests.
#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub struct FnFilter<F>(pub F);

#[async_trait]
impl<F> Filter for FnFilter<F>
where
    F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
{
    #[inline]
    async fn filter(&self, req: &mut Request, path: &mut PathState) -> bool {
        self.0(req, path)
    }
}

impl<F> fmt::Debug for FnFilter<F> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "fn:fn")
    }
}

/// Filter request by uri scheme.
#[inline]
pub fn scheme(scheme: Scheme) -> SchemeFilter {
    SchemeFilter::new(scheme)
}

/// Filter request by uri hostname.
#[inline]
pub fn host(host: impl Into<String>) -> HostFilter {
    HostFilter::new(host)
}

/// Filter request by uri port.
#[inline]
pub fn port(port: u16) -> PortFilter {
    PortFilter::new(port)
}

/// Filter request use `PathFilter`.
#[inline]
pub fn path(path: impl Into<String>) -> PathFilter {
    PathFilter::new(path)
}
/// Filter request, only allow get method.
#[inline]
pub fn get() -> MethodFilter {
    MethodFilter(Method::GET)
}
/// Filter request, only allow head method.
#[inline]
pub fn head() -> MethodFilter {
    MethodFilter(Method::HEAD)
}
/// Filter request, only allow options method.
#[inline]
pub fn options() -> MethodFilter {
    MethodFilter(Method::OPTIONS)
}
/// Filter request, only allow post method.
#[inline]
pub fn post() -> MethodFilter {
    MethodFilter(Method::POST)
}
/// Filter request, only allow patch method.
#[inline]
pub fn patch() -> MethodFilter {
    MethodFilter(Method::PATCH)
}
/// Filter request, only allow put method.
#[inline]
pub fn put() -> MethodFilter {
    MethodFilter(Method::PUT)
}

/// Filter request, only allow delete method.
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

    #[tokio::test]
    async fn test_opts() {
        fn has_one(_req: &mut Request, path: &mut PathState) -> bool {
            path.parts.contains(&"one".into())
        }
        fn has_two(_req: &mut Request, path: &mut PathState) -> bool {
            path.parts.contains(&"two".into())
        }

        let one_filter = FnFilter(has_one);
        let two_filter = FnFilter(has_two);

        let mut req = Request::default();
        let mut path_state = PathState::new("http://localhost/one");
        assert!(one_filter.filter(&mut req, &mut path_state).await);
        assert!(!two_filter.filter(&mut req, &mut path_state).await);
        assert!(
            one_filter
                .or_else(has_two)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            one_filter
                .or(two_filter)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            !one_filter
                .and_then(has_two)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            !one_filter
                .and(two_filter)
                .filter(&mut req, &mut path_state)
                .await
        );

        let mut path_state = PathState::new("http://localhost/one/two");
        assert!(one_filter.filter(&mut req, &mut path_state).await);
        assert!(two_filter.filter(&mut req, &mut path_state).await);
        assert!(
            one_filter
                .or_else(has_two)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            one_filter
                .or(two_filter)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            one_filter
                .and_then(has_two)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            one_filter
                .and(two_filter)
                .filter(&mut req, &mut path_state)
                .await
        );

        let mut path_state = PathState::new("http://localhost/two");
        assert!(!one_filter.filter(&mut req, &mut path_state).await);
        assert!(two_filter.filter(&mut req, &mut path_state).await);
        assert!(
            one_filter
                .or_else(has_two)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            one_filter
                .or(two_filter)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            !one_filter
                .and_then(has_two)
                .filter(&mut req, &mut path_state)
                .await
        );
        assert!(
            !one_filter
                .and(two_filter)
                .filter(&mut req, &mut path_state)
                .await
        );
    }
}
