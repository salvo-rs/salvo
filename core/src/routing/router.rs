use futures::future::{BoxFuture, FutureExt};
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::future::Future;
use std::sync::Arc;

use super::{AnyFilter, Filter, PathState};
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
    pub async fn detect(&self, request: &mut Request, path: &mut PathState) -> Option<DetectMatched> {
        let match_cursor = path.match_cursor;
        if self.filter.execute(request, path).await {
            if let Some(handler) = self.handler.clone() {
                // if !self.children.is_empty() {
                //     for child in &self.children {
                //         if let Some(dm) = child.detect(request, path).await {
                //             return Some(DetectMatched {
                //                 befores: Vec::from([&self.befores[..], &dm.befores[..]].concat()),
                //                 afters: Vec::from([&self.afters[..], &dm.befores[..]].concat()),
                //                 handler: dm.handler.clone(),
                //             });
                //         }
                //     }
                // }
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

    pub fn handle<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.handler = Some(Arc::new(handler));
        self
    }
}
