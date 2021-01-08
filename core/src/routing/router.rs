// use std::fmt::{self, Debug};
use std::sync::Arc;

use super::filter;
use super::{Filter, PathFilter, PathState};
use crate::http::Request;
use crate::Handler;

pub struct Router {
    pub children: Vec<Router>,
    pub filter: Option<Box<dyn Filter>>,
    pub handler: Option<Arc<dyn Handler>>,
    pub befores: Vec<Arc<dyn Handler>>,
    pub afters: Vec<Arc<dyn Handler>>,
}
pub struct DetectMatched {
    pub handler: Arc<dyn Handler>,
    pub befores: Vec<Arc<dyn Handler>>,
    pub afters: Vec<Arc<dyn Handler>>,
}

// impl Debug for Router {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{{ : '{}'}}", &self)
//     }
// }

impl Router {
    pub fn new() -> Router {
        Router {
            children: Vec::new(),
            befores: Vec::new(),
            afters: Vec::new(),
            filter: None,
            handler: None,
        }
    }
    pub fn detect(&self, request: &mut Request, path: &mut PathState) -> Option<DetectMatched> {
        let match_cursor = path.match_cursor;
        if let Some(filter) = &self.filter {
            if !filter.filter(request, path) {
                path.match_cursor = match_cursor;
                return None;
            }
        }
        if let Some(handler) = self.handler.clone() {
            if path.segements.len() <= path.match_cursor {
                return Some(DetectMatched {
                    befores: self.befores.clone(),
                    afters: self.afters.clone(),
                    handler: handler.clone(),
                });
            }
        }
        if !self.children.is_empty() {
            for child in &self.children {
                if let Some(dm) = child.detect(request, path) {
                    return Some(DetectMatched {
                        befores: Vec::from([&self.befores[..], &dm.befores[..]].concat()),
                        afters: Vec::from([&self.afters[..], &dm.befores[..]].concat()),
                        handler: dm.handler.clone(),
                    });
                }
            }
        }
        path.match_cursor = match_cursor;
        return None;
    }

    pub fn push(mut self, router: Router) -> Self {
        self.children.push(router);
        self
    }
    pub fn before<H: Handler>(mut self, handler: H) -> Self {
        self.befores.push(Arc::new(handler));
        self
    }
    pub fn after<H: Handler>(mut self, handler: H) -> Self {
        self.afters.push(Arc::new(handler));
        self
    }
    pub fn path(self, path: impl Into<String>) -> Self {
        self.filter(PathFilter::new(path))
    }
    pub fn filter(mut self, filter: impl Filter) -> Self {
        if let Some(exist_filter) = self.filter {
            self.filter = Some(exist_filter);
        } else {
            self.filter = Some(Box::new(filter));
        }
        self
    }

    pub fn handle<H: Handler>(mut self, handler: H) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }
    pub fn get<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::get()).handle(handler))
    }

    pub fn post<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::post()).handle(handler))
    }

    pub fn put<H: Handler, I: AsRef<str>>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::put()).handle(handler))
    }

    pub fn delete<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::delete()).handle(handler))
    }

    pub fn head<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::head()).handle(handler))
    }

    pub fn patch<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::patch()).handle(handler))
    }

    pub fn options<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::options()).handle(handler))
    }
}
