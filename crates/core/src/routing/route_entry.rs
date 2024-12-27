use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use http::Method;
use indexmap::IndexMap;

use crate::http::{Request, Response};
use crate::{Depot, Handler};

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteEntry {
    pub(crate) method: Method,
    pub(crate) path_parts: Vec<String>,
}
impl RouteEntry {
    /// Create new `RouteEntry`.
    #[inline]
    pub fn new(method: Method) -> Self {
        RouteEntry {
            method,
            path_parts: Vec::new(),
        }
    }

    pub fn all_rest(&self) -> Option<Cow<'_, str>> {
        if let Some(picked) = self.pick() {
            if self.cursor.0 >= self.parts.len() - 1 {
                if self.end_slash {
                    Some(Cow::Owned(format!("{picked}/")))
                } else {
                    Some(Cow::Borrowed(picked))
                }
            } else {
                let last = self.parts[self.cursor.0 + 1..].join("/");
                if self.end_slash {
                    Some(Cow::Owned(format!("{picked}/{last}/")))
                } else {
                    Some(Cow::Owned(format!("{picked}/{last}")))
                }
            }
        } else {
            None
        }
    }
}
