use std::fmt::{self, Formatter};

use crate::http::uri::Scheme;
use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

/// Filter by request method
#[derive(Clone, PartialEq, Eq)]
pub struct MethodFilter(pub Method);

impl Filter for MethodFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.method() == self.0
    }
}

impl fmt::Debug for MethodFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "method:{:?}", self.0)
    }
}

/// Filter by request uri scheme.
#[derive(Clone, PartialEq, Eq)]
pub struct SchemeFilter(pub Scheme, pub bool);

impl Filter for SchemeFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri().scheme().map(|s| s == &self.0).unwrap_or(self.1)
    }
}
impl fmt::Debug for SchemeFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "scheme:{:?}", self.0)
    }
}

/// Filter by request uri host.
#[derive(Clone, PartialEq, Eq)]
pub struct HostFilter(pub String, pub bool);

impl Filter for HostFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri().host().map(|h| h == self.0).unwrap_or(self.1)
    }
}

impl fmt::Debug for HostFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "host:{:?}", self.0)
    }
}

/// Filter by request uri host.
#[derive(Clone, PartialEq, Eq)]
pub struct PortFilter(pub u16, pub bool);

impl Filter for PortFilter {
    #[inline]
    fn filter(&self, req: &mut Request, _state: &mut PathState) -> bool {
        req.uri().port_u16().map(|p| p == self.0).unwrap_or(self.1)
    }
}

impl fmt::Debug for PortFilter {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "port:{:?}", self.0)
    }
}
