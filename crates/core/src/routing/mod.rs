//! Routing and filters.
//!
//! # What is router
//!
//! Router can route HTTP requests to different handlers. This is a basic and key feature in salvo.
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
//! Router::with_path("writers/{id}").get(show_writer).patch(edit_writer).delete(delete_writer);
//! Router::with_path("writers/{id}/articles").get(list_writer_articles);
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
//!         Router::with_path("{id}")
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
//!             .push(Router::with_path("{id}").get(show_article))
//!             .then(|router|{
//!                 if admin_mode() {
//!                     router.post(create_article).push(
//!                         Router::with_path("{id}").patch(update_article).delete(delete_writer)
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
//! In the previous source code, `{id}` is a param definition. We can access its value via Request instance:
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
//! `{id}` matches a fragment in the path, under normal circumstances, the article `id` is just a number, which we can
//! use regular expressions to restrict `id` matching rules, `r"{id|\d+}"`.
//!
//! For numeric characters there is an easier way to use `{id:num}`, the specific writing is:
//!
//! - `{id:num}`, matches any number of numeric characters;
//! - `{id:num[10]}`, only matches a certain number of numeric characters, where 10 means that the match only matches
//!   10 numeric characters;
//! - `{id:num(..10)}` means matching 1 to 9 numeric characters;
//! - `{id:num(3..10)}` means matching 3 to 9 numeric characters;
//! - `{id:num(..=10)}` means matching 1 to 10 numeric characters;
//! - `{id:num(3..=10)}` means match 3 to 10 numeric characters;
//! - `{id:num(10..)}` means to match at least 10 numeric characters.
//!
//! You can also use `{**}`, `{*+*}` or `{*?}` to match all remaining path fragments.
//! In order to make the code more readable, you can also add appropriate name to make the path semantics more clear,
//! for example: `{**file_path}`.
//!
//! It is allowed to combine multiple expressions to match the same path segment,
//! such as `/articles/article_{id:num}/`, `/images/{name}.{ext}`.
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
//!         Router::with_path("{id}")
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
//!             .push(Router::with_path("{id}").patch(edit_writer).delete(delete_writer)),
//!     )
//!     .push(
//!         Router::new().path("writers").get(list_writers).push(
//!             Router::with_path("{id}")
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
//!             .push(Router::new().path("{id}").get(show_article)),
//!     )
//!     .push(
//!         Router::new()
//!             .path("articles")
//!             .hoop(auth_check)
//!             .post(list_articles)
//!             .push(Router::new().path("{id}").patch(edit_article).delete(delete_article)),
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
//! Router::with_path("articles/{id}").get(show_article);
//! Router::with_path("files/{**rest_path}").get(serve_file);
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
//! Router::with_path("/articles/{id|[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}}");
//! Router::with_path("/users/{id|[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}}");
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
//!         .push(Router::with_path("/articles/{id:guid}").get(show_article))
//!         .push(Router::with_path("/users/{id:guid}").get(show_user));
//! }
//! ```
//!
//! You only need to register once, and then you can directly match the GUID through the simple writing method as
//! `{id:guid}`, which simplifies the writing of the code.

pub mod filters;
pub use filters::*;
mod router;
pub use router::Router;

mod path_params;
pub use path_params::PathParams;
mod path_state;
pub use path_state::PathState;
mod flow_ctrl;
pub use flow_ctrl::FlowCtrl;

use std::sync::Arc;

use crate::Handler;

#[doc(hidden)]
pub struct DetectMatched {
    pub hoops: Vec<Arc<dyn Handler>>,
    pub goal: Arc<dyn Handler>,
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

#[inline]
fn decode_url_path_safely(path: &str) -> String {
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .to_string()
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

        assert!(
            access(&service, "127.0.0.1")
                .await
                .contains("404: Not Found")
        );
        assert_eq!(access(&service, "localhost").await, "Hello World");
    }

    #[tokio::test]
    async fn test_matched_path() {
        #[handler]
        async fn alice1(req: &mut Request) {
            assert_eq!(req.matched_path(), "open/alice1");
        }
        #[handler]
        async fn bob1(req: &mut Request) {
            assert_eq!(req.matched_path(), "open/alice1/bob1");
        }

        #[handler]
        async fn alice2(req: &mut Request) {
            assert_eq!(req.matched_path(), "open/alice2");
        }
        #[handler]
        async fn bob2(req: &mut Request) {
            assert_eq!(req.matched_path(), "open/alice2/bob2");
        }

        #[handler]
        async fn alice3(req: &mut Request) {
            assert_eq!(req.matched_path(), "alice3");
        }
        #[handler]
        async fn bob3(req: &mut Request) {
            assert_eq!(req.matched_path(), "alice3/bob3");
        }

        let router = Router::new()
            .push(
                Router::with_path("open").push(
                    Router::with_path("alice1")
                        .get(alice1)
                        .push(Router::with_path("bob1").get(bob1)),
                ),
            )
            .push(
                Router::with_path("open").push(
                    Router::with_path("alice2")
                        .get(alice2)
                        .push(Router::with_path("bob2").get(bob2)),
                ),
            )
            .push(
                Router::with_path("alice3")
                    .get(alice3)
                    .push(Router::with_path("bob3").get(bob3)),
            );
        let service = Service::new(router);

        async fn access(service: &Service, path: &str) {
            TestClient::get(format!("http://127.0.0.1/{}", path))
                .send(service)
                .await;
        }
        access(&service, "/open/alice1").await;
        access(&service, "/open/alice1/bob1").await;
        access(&service, "/open/alice2").await;
        access(&service, "/open/alice2/bob2").await;
        access(&service, "/alice3").await;
        access(&service, "/alice1/bob3").await;
    }
}
