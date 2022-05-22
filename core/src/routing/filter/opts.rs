use std::fmt::{self, Formatter};

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for Or<T, U>
where
    T: Filter + Send,
    U: Filter + Send,
{
    #[inline]
    fn filter(&self, req: &mut Request, state: &mut PathState) -> bool {
        if self.first.filter(req, state) {
            true
        } else {
            self.second.filter(req, state)
        }
    }
}

#[derive(Clone, Copy)]
pub struct OrElse<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> Filter for OrElse<T, F>
where
    T: Filter,
    F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
{
    #[inline]
    fn filter(&self, req: &mut Request, state: &mut PathState) -> bool {
        if self.filter.filter(req, state) {
            true
        } else {
            (self.callback)(req, state)
        }
    }
}

impl<T, F> fmt::Debug for OrElse<T, F> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "opt:or_else")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct And<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for And<T, U>
where
    T: Filter,
    U: Filter,
{
    #[inline]
    fn filter(&self, req: &mut Request, state: &mut PathState) -> bool {
        if !self.first.filter(req, state) {
            false
        } else {
            self.second.filter(req, state)
        }
    }
}

#[derive(Clone, Copy)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> Filter for AndThen<T, F>
where
    T: Filter,
    F: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
{
    #[inline]
    fn filter(&self, req: &mut Request, state: &mut PathState) -> bool {
        if !self.filter.filter(req, state) {
            false
        } else {
            (self.callback)(req, state)
        }
    }
}

impl<T, F> fmt::Debug for AndThen<T, F> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "opt:and_then")
    }
}
