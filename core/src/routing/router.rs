use std::fmt::{self, Debug};
use std::sync::Arc;

use super::filter;
use super::{AnyFilter, Filter, PathFilter, PathState};
use crate::http::Request;
use crate::Handler;

pub struct Router {
    pub children: Vec<Router>,
    pub filter: Box<dyn Filter>,
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
            filter: Box::new(AnyFilter),
            handler: None,
        }
    }

    pub fn push(&mut self, router: Router) -> &mut Router {
        self.children.push(router);
        self
    }

    pub fn before<H: Handler>(&mut self, handler: H) -> &mut Router {
        let handler = Arc::new(handler);
        self.befores.push(handler.clone());
        self
    }
    pub fn after<H: Handler>(&mut self, handler: H) -> &mut Router {
        let handler = Arc::new(handler);
        self.afters.push(handler.clone());
        self
    }
    pub fn detect(&self, request: &mut Request, path: &mut PathState) -> Option<DetectMatched> {
        let match_cursor = path.match_cursor;
        if self.filter.execute(request, path) {
            if let Some(handler) = self.handler.clone() {
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
                Some(DetectMatched {
                    befores: self.befores.clone(),
                    afters: self.afters.clone(),
                    handler: handler.clone(),
                })
            } else {
                path.match_cursor = match_cursor;
                None
            }
        } else {
            path.match_cursor = match_cursor;
            None
        }
    }
    pub fn path(&mut self, path: impl Into<String>) -> &mut Router {
        self.filter(PathFilter::new(path))
    }
    pub fn filter(&mut self, filter: impl Filter) -> &mut Router {
        if self.filter == AnyFilter {
            self.filter = filter;
        } else {
            self.filter = self.filter.and(filter);
        }
        self
    }

    pub fn handle<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.handler = Some(Arc::new(handler));
        self
    }
    /// Like route, but specialized to the `Get` method.
    pub fn get<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::get()).handle(handler))
    }

    /// Like route, but specialized to the `Post` method.
    pub fn post<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::post()).handle(handler))
    }

    /// Like route, but specialized to the `Put` method.
    pub fn put<H: Handler, I: AsRef<str>>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::put()).handle(handler))
    }

    /// Like route, but specialized to the `Delete` method.
    pub fn delete<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::delete()).handle(handler))
    }

    /// Like route, but specialized to the `Head` method.
    pub fn head<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::head()).handle(handler))
    }

    /// Like route, but specialized to the `Patch` method.
    pub fn patch<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::patch()).handle(handler))
    }

    /// Like route, but specialized to the `Options` method.
    pub fn options<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.push(Router::new().filter(filter::options()).handle(handler))
    }
}
