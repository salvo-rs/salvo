//! Routing and filters.
//!
//! # What is router
//!
//! Router can route http requests to different handlers. This is a basic and key feature in salvo.
//!
//! The interior of [`Router`] is actually composed of a series of filters. When a request comes, the route will
//! test itself and its descendants in order to see if they can match the request in the order they were added, and
//! then execute the middleware on the entire chain formed by the route and its descendants in sequence. If the
//! status of [`Response`] is set to error (4XX, 5XX) or jump (3XX) during processing, the subsequent middleware and
//! [`Handler`] will be skipped. You can also manually adjust `ctrl.skip_rest()` to skip subsequent middleware and
//! [`Handler`].
//!
//! # Write in flat way
//!
//! We can write routers in flat way, like this:
//!
//! ```rust
//! # use salvo_core::prelude::*;
//!
//! # #[handler]
//! # async fn create_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn show_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn list_writers(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn edit_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn delete_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn list_writer_articles(res: &mut Response) {
//! # }
//! Router::with_path("writers").get(list_writers).post(create_writer);
//! Router::with_path("writers/<id>").get(show_writer).patch(edit_writer).delete(delete_writer);
//! Router::with_path("writers/<id>/articles").get(list_writer_articles);
//! ```
//!
//! # Write in tree way
//!
//! We can write router like a tree, this is also the recommended way:
//!
//! ```rust
//! # use salvo_core::prelude::*;
//!
//! # #[handler]
//! # async fn create_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn show_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn list_writers(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn edit_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn delete_writer(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn list_writer_articles(res: &mut Response) {
//! # }
//! Router::with_path("writers")
//!     .get(list_writers)
//!     .post(create_writer)
//!     .push(
//!         Router::with_path("<id>")
//!             .get(show_writer)
//!             .patch(edit_writer)
//!             .delete(delete_writer)
//!             .push(Router::with_path("articles").get(list_writer_articles)),
//!     );
//! ```
//!
//! This form of definition can make the definition of router clear and simple for complex projects.
//!
//! There are many methods in `Router` that will return to `Self` after being called, so as to write code in a chain.
//! Sometimes, you need to decide how to route according to certain conditions, and the `Router` also provides `then`
//! function, which is also easy to use:
//!
//! ```rust
//! # use salvo_core::prelude::*;
//!
//! # #[handler]
//! # async fn list_articles(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn show_article(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn create_article(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn update_article(res: &mut Response) {
//! # }
//! # #[handler]
//! # async fn delete_writer(res: &mut Response) {
//! # }
//! fn admin_mode() -> bool { true };
//! Router::new()
//!     .push(
//!         Router::with_path("articles")
//!             .get(list_articles)
//!             .push(Router::with_path("<id>").get(show_article))
//!             .then(|router|{
//!                 if admin_mode() {
//!                     router.post(create_article).push(
//!                         Router::with_path("<id>").patch(update_article).delete(delete_writer)
//!                     )
//!                 } else {
//!                     router
//!                 }
//!             }),
//!     );
//! ```
//!
//! This example represents that only when the server is in `admin_mode`, routers such as creating articles, editing
//! and deleting articles will be added.
//!
//! # Get param in routers
//!
//! In the previous source code, `<id>` is a param definition. We can access its value via Request instance:
//!
//! ```rust
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn show_writer(req: &mut Request) {
//!     let id = req.param::<i64>("id").unwrap();
//! }
//! ```
//!
//! `<id>` matches a fragment in the path, under normal circumstances, the article `id` is just a number, which we can
//! use regular expressions to restrict `id` matching rules, `r"<id:/\d+/>"`.
//!
//! For numeric characters there is an easier way to use `<id:num>`, the specific writing is:
//!
//! - `<id:num>`, matches any number of numeric characters;
//! - `<id:num[10]>`, only matches a certain number of numeric characters, where 10 means that the match only matches
//!   10 numeric characters;
//! - `<id:num(..10)>` means matching 1 to 9 numeric characters;
//! - `<id:num(3..10)>` means matching 3 to 9 numeric characters;
//! - `<id:num(..=10)>` means matching 1 to 10 numeric characters;
//! - `<id:num(3..=10)>` means match 3 to 10 numeric characters;
//! - `<id:num(10..)>` means to match at least 10 numeric characters.
//!
//! You can also use `<**>`, `<*+*>` or `<*?>` to match all remaining path fragments.
//! In order to make the code more readable, you can also add appropriate name to make the path semantics more clear,
//! for example: `<**file_path>`.
//!
//! It is allowed to combine multiple expressions to match the same path segment,
//! such as `/articles/article_<id:num>/`, `/images/<name>.<ext>`.
//!
//! # Add middlewares
//!
//! Middleware can be added via `hoop` method.
//!
//! ```rust
//! # use salvo_core::prelude::*;
//!
//! # #[handler] fn create_writer() {}
//! # #[handler] fn show_writer() {}
//! # #[handler] fn list_writers() {}
//! # #[handler] fn edit_writer() {}
//! # #[handler] fn delete_writer() {}
//! # #[handler] fn list_writer_articles() {}
//! # #[handler] fn check_authed() {}
//! Router::new()
//!     .hoop(check_authed)
//!     .path("writers")
//!     .get(list_writers)
//!     .post(create_writer)
//!     .push(
//!         Router::with_path("<id>")
//!             .get(show_writer)
//!             .patch(edit_writer)
//!             .delete(delete_writer)
//!             .push(Router::with_path("articles").get(list_writer_articles)),
//!     );
//! ```
//!
//! In this example, the root router has a middleware to check current user is authenticated. This middleware will
//! affect the root router and its descendants.
//!
//! If we don't want to check user is authed when current user view writer informations and articles. We can write
//! router like this:
//!
//! ```rust
//! # use salvo_core::prelude::*;
//!
//! # #[handler] fn create_writer() {}
//! # #[handler] fn show_writer() {}
//! # #[handler] fn list_writers() {}
//! # #[handler] fn edit_writer() {}
//! # #[handler] fn delete_writer() {}
//! # #[handler] fn list_writer_articles() {}
//! # #[handler] fn check_authed() {}
//! Router::new()
//!     .push(
//!         Router::new()
//!             .hoop(check_authed)
//!             .path("writers")
//!             .post(create_writer)
//!             .push(Router::with_path("<id>").patch(edit_writer).delete(delete_writer)),
//!     )
//!     .push(
//!         Router::new().path("writers").get(list_writers).push(
//!             Router::with_path("<id>")
//!                 .get(show_writer)
//!                 .push(Router::with_path("articles").get(list_writer_articles)),
//!         ),
//!     );
//! ```
//!
//! Although there are two routers have the same `path("writers")`, they can still be added to the same parent route
//! at the same time.
//!
//! # Filters
//!
//! Many methods in `Router` return to themselves in order to easily implement chain writing. Sometimes, in some cases,
//! you need to judge based on conditions before you can add routing. Routing also provides some convenience Method,
//! simplify code writing.
//!
//! `Router` uses the filter to determine whether the route matches. The filter supports logical operations and or.
//! Multiple filters can be added to a route. When all the added filters match, the route is matched successfully.
//!
//! It should be noted that the URL collection of the website is a tree structure, and this structure is not equivalent
//! to the tree structure of `Router`. A node of the URL may correspond to multiple `Router`. For example, some paths
//! under the `articles/` path require login, and some paths do not require login. Therefore, we can put the same login
//! requirements under a `Router`, and on top of them Add authentication middleware on `Router`.
//!
//! In addition, you can access it without logging in and put it under another route without authentication middleware:
//!
//! ```rust
//! # use salvo_core::prelude::*;
//!
//! # #[handler] fn list_articles() {}
//! # #[handler] fn show_article() {}
//! # #[handler] fn edit_article() {}
//! # #[handler] fn delete_article() {}
//! # #[handler] fn auth_check() {}
//! Router::new()
//!     .push(
//!         Router::new()
//!             .path("articles")
//!             .get(list_articles)
//!             .push(Router::new().path("<id>").get(show_article)),
//!     )
//!     .push(
//!         Router::new()
//!             .path("articles")
//!             .hoop(auth_check)
//!             .post(list_articles)
//!             .push(Router::new().path("<id>").patch(edit_article).delete(delete_article)),
//!     );
//! ```
//!
//! Router is used to filter requests, and then send the requests to different Handlers for processing.
//!
//! The most commonly used filtering is `path` and `method`. `path` matches path information; `method` matches
//! the requested Method.
//!
//! We can use `and`, `or` to connect between filter conditions, for example:
//!
//! ```rust
//! use salvo_core::prelude::*;
//! use salvo_core::routing::*;
//!
//! Router::new().filter(filters::path("hello").and(filters::get()));
//! ```
//!
//! ## Path filter
//!
//! The filter is based on the request path is the most frequently used. Parameters can be defined in the path
//! filter, such as:
//!
//! ```rust
//! use salvo_core::prelude::*;
//!
//! # #[handler] fn show_article() {}
//! # #[handler] fn serve_file() {}
//! Router::with_path("articles/<id>").get(show_article);
//! Router::with_path("files/<**rest_path>").get(serve_file);
//! ```
//!
//! In `Handler`, it can be obtained through the `get_param` function of the `Request` object:
//!
//! ```rust
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! fn show_article(req: &mut Request) {
//!     let article_id = req.param::<i64>("id");
//! }
//!
//! #[handler]
//! fn serve_file(req: &mut Request) {
//!     let rest_path = req.param::<i64>("rest_path");
//! }
//! ```
//!
//! ## Method filter
//!
//! Filter requests based on the `HTTP` request's `Method`, for example:
//!
//! ```rust
//! use salvo_core::prelude::*;
//!
//! # #[handler] fn show_article() {}
//! # #[handler] fn update_article() {}
//! # #[handler] fn delete_article() {}
//! Router::new().get(show_article).patch(update_article).delete(delete_article);
//! ```
//!
//! Here `get`, `patch`, `delete` are all Method filters. It is actually equivalent to:
//!
//! ```rust
//! use salvo_core::prelude::*;
//! use salvo_core::routing::*;
//! # #[handler] fn show_article() {}
//! # #[handler] fn update_article() {}
//! # #[handler] fn delete_article() {}
//!
//! let show_router = Router::with_filter(filters::get()).goal(show_article);
//! let update_router = Router::with_filter(filters::patch()).goal(update_article);
//! let delete_router = Router::with_filter(filters::get()).goal(delete_article);
//! Router::new().push(show_router).push(update_router).push(delete_router);
//! ```
//!
//! ## Custom Wisp
//!
//! For some frequently-occurring matching expressions, we can name a short name by
//! `PathFilter::register_wisp_regex` or `PathFilter::register_wisp_builder`. For example, GUID format is often used
//! in paths appears, normally written like this every time a match is required:
//!
//! ```rust
//! use salvo_core::prelude::*;
//!
//! Router::with_path("/articles/<id:/[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}/>");
//! Router::with_path("/users/<id:/[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}/>");
//! ```
//!
//! However, writing this complex regular expression every time is prone to errors and hard-coding the regex is not
//! ideal. We could separate the regex into its own Regex variable like so:
//!
//! ```rust
//! use salvo_core::prelude::*;
//! use salvo_core::routing::filters::PathFilter;
//!
//! # #[handler] fn show_article() {}
//! # #[handler] fn show_user() {}
//!
//! #[tokio::main]
//! async fn main() {
//!     let guid = regex::Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap();
//!     PathFilter::register_wisp_regex("guid", guid);
//!     Router::new()
//!         .push(Router::with_path("/articles/<id:guid>").get(show_article))
//!         .push(Router::with_path("/users/<id:guid>").get(show_user));
//! }
//! ```
//!
//! You only need to register once, and then you can directly match the GUID through the simple writing method as
//! `<id:guid>`, which simplifies the writing of the code.

pub mod filters;
pub use filters::*;
mod router;
pub use router::Router;

use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::http::{Request, Response};
use crate::{Depot, Handler};

#[doc(hidden)]
pub struct DetectMatched {
    pub hoops: Vec<Arc<dyn Handler>>,
    pub goal: Arc<dyn Handler>,
}

/// The path parameters.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct PathParams {
    inner: IndexMap<String, String>,
    greedy: bool,
}
impl Deref for PathParams {
    type Target = IndexMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl PathParams {
    /// Create new `PathParams`.
    pub fn new() -> Self {
        PathParams::default()
    }
    /// If there is a wildcard param, it's value is `true`.
    pub fn greedy(&self) -> bool {
        self.greedy
    }
    /// Get the last param starts with '*', for example: <**rest>, <*?rest>.
    pub fn tail(&self) -> Option<&str> {
        if self.greedy {
            self.inner.last().map(|(_, v)| &**v)
        } else {
            None
        }
    }

    /// Insert new param.
    pub fn insert(&mut self, name: &str, value: String) {
        #[cfg(debug_assertions)]
        {
            if self.greedy {
                panic!("only one wildcard param is allowed and it must be the last one.");
            }
        }
        if name.starts_with('*') {
            self.inner.insert(split_wild_name(name).1.to_owned(), value);
            self.greedy = true;
        } else {
            self.inner.insert(name.to_owned(), value);
        }
    }
}

pub(crate) fn split_wild_name(name: &str) -> (&str, &str) {
    if name.starts_with("*+") || name.starts_with("*?") || name.starts_with("**") {
        (&name[0..2], &name[2..])
    } else if let Some(stripped) = name.strip_prefix('*') {
        ("*", stripped)
    } else {
        ("", name)
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathState {
    pub(crate) parts: Vec<String>,
    /// (row, col), row is the index of parts, col is the index of char in the part.
    pub(crate) cursor: (usize, usize),
    pub(crate) params: PathParams,
    pub(crate) end_slash: bool, // For rest match, we want include the last slash.
    pub(crate) once_ended: bool, // Once it has ended, used to determine whether the error code returned is 404 or 405.
}
impl PathState {
    /// Create new `PathState`.
    #[inline]
    pub fn new(url_path: &str) -> Self {
        let end_slash = url_path.ends_with('/');
        let parts = url_path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter_map(|p| {
                if !p.is_empty() {
                    Some(decode_url_path_safely(p))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        PathState {
            parts,
            cursor: (0, 0),
            params: PathParams::new(),
            end_slash,
            once_ended: false,
        }
    }

    #[inline]
    pub fn pick(&self) -> Option<&str> {
        match self.parts.get(self.cursor.0) {
            None => None,
            Some(part) => {
                if self.cursor.1 >= part.len() {
                    let row = self.cursor.0 + 1;
                    self.parts.get(row).map(|s| &**s)
                } else {
                    Some(&part[self.cursor.1..])
                }
            }
        }
    }

    #[inline]
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

    #[inline]
    pub fn forward(&mut self, steps: usize) {
        let mut steps = steps + self.cursor.1;
        while let Some(part) = self.parts.get(self.cursor.0) {
            if part.len() > steps {
                self.cursor.1 = steps;
                return;
            } else {
                steps -= part.len();
                self.cursor = (self.cursor.0 + 1, 0);
            }
        }
    }

    #[inline]
    pub fn is_ended(&self) -> bool {
        self.cursor.0 >= self.parts.len()
    }
}

#[inline]
fn decode_url_path_safely(path: &str) -> String {
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .to_string()
}

/// Control the flow of execute handlers.
///
/// When a request is coming, [`Router`] will detect it and get the matched router.
/// And then salvo will collect all handlers (including added as middlewares) from the matched router tree.
/// All handlers in this list will executed one by one.
///
/// Each handler can use `FlowCtrl` to control execute flow, let the flow call next handler or skip all rest handlers.
///
/// **NOTE**: When `Response`'s status code is set, and the status code [`Response::is_stamped()`] is returns false,
/// all rest handlers will skipped.
///
/// [`Router`]: crate::routing::Router
#[derive(Default)]
pub struct FlowCtrl {
    catching: Option<bool>,
    is_ceased: bool,
    pub(crate) cursor: usize,
    pub(crate) handlers: Vec<Arc<dyn Handler>>,
}

impl FlowCtrl {
    /// Create new `FlowCtrl`.
    #[inline]
    pub fn new(handlers: Vec<Arc<dyn Handler>>) -> Self {
        FlowCtrl {
            catching: None,
            is_ceased: false,
            cursor: 0,
            handlers,
        }
    }
    /// Has next handler.
    #[inline]
    pub fn has_next(&self) -> bool {
        self.cursor < self.handlers.len() // && !self.handlers.is_empty()
    }

    /// Call next handler. If get next handler and executed, returns `true``, otherwise returns `false`.
    ///
    /// **NOTE**: If response status code is error or is redirection, all reset handlers will be skipped.
    #[inline]
    pub async fn call_next(
        &mut self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
    ) -> bool {
        if self.catching.is_none() {
            self.catching = Some(res.is_stamped());
        }
        if !self.catching.unwrap_or_default() && res.is_stamped() {
            self.skip_rest();
            return false;
        }
        let mut handler = self.handlers.get(self.cursor).cloned();
        if handler.is_none() {
            false
        } else {
            while let Some(h) = handler.take() {
                self.cursor += 1;
                h.handle(req, depot, res, self).await;
                if !self.catching.unwrap_or_default() && res.is_stamped() {
                    self.skip_rest();
                    return true;
                } else if self.has_next() {
                    handler = self.handlers.get(self.cursor).cloned();
                }
            }
            true
        }
    }

    /// Skip all reset handlers.
    #[inline]
    pub fn skip_rest(&mut self) {
        self.cursor = self.handlers.len()
    }

    /// Check is `FlowCtrl` ceased.
    ///
    /// **NOTE**: If handler is used as middleware, it should use `is_ceased` to check is flow ceased.
    /// If `is_ceased` returns `true`, the handler should skip the following logic.
    #[inline]
    pub fn is_ceased(&self) -> bool {
        self.is_ceased
    }
    /// Cease all following logic.
    ///
    /// **NOTE**: This function will mark is_ceased as `true`, but whether the subsequent logic can be skipped
    /// depends on whether the middleware correctly checks is_ceased and skips the subsequent logic.
    #[inline]
    pub fn cease(&mut self) {
        self.skip_rest();
        self.is_ceased = true;
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_custom_filter() {
        #[handler]
        async fn hello() -> &'static str {
            "Hello World"
        }

        let router = Router::new()
            .filter_fn(|req, _| {
                let host = req.uri().host().unwrap_or_default();
                host == "localhost"
            })
            .get(hello);
        let service = Service::new(router);

        async fn access(service: &Service, host: &str) -> String {
            TestClient::get(format!("http://{}/", host))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert!(access(&service, "127.0.0.1")
            .await
            .contains("404: Not Found"));
        assert_eq!(access(&service, "localhost").await, "Hello World");
    }
}
