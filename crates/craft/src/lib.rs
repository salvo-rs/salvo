#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Modular handler crafting for Salvo web framework.
//!
//! This crate provides the `#[craft]` attribute macro that enables a more
//! ergonomic way to define handlers as methods on structs, allowing for
//! better code organization and state sharing.
//!
//! # Overview
//!
//! Instead of writing standalone handler functions, you can organize related
//! handlers as methods on a struct, with shared state accessible via `self`:
//!
//! ```
//! use salvo::oapi::extract::PathParam;
//! use salvo::prelude::*;
//! use salvo_craft::craft;
//!
//! #[derive(Clone, Debug)]
//! pub struct UserService {
//!     prefix: &'static str,
//! }
//!
//! #[craft]
//! impl UserService {
//!     #[craft(handler)]
//!     fn get_user(&self, id: PathParam<i64>) -> String {
//!         format!("{} user {}", self.prefix, *id)
//!     }
//!
//!     #[craft(handler)]
//!     fn list_users(&self) -> &'static str {
//!         "listing users"
//!     }
//! }
//!
//! let service = UserService { prefix: "hello" };
//! let _router = Router::new()
//!     .push(Router::with_path("users").get(service.list_users()))
//!     .push(Router::with_path("users/<id>").get(service.get_user()));
//! ```
//!
//! # Usage
//!
//! ## Basic Handler
//!
//! Use `#[craft(handler)]` to mark a method as a handler:
//!
//! ```
//! use salvo::prelude::*;
//! use salvo_craft::craft;
//!
//! #[derive(Clone, Debug)]
//! pub struct MyService;
//!
//! #[craft]
//! impl MyService {
//!     #[craft(handler)]
//!     fn hello(&self) -> &'static str {
//!         "hello, world"
//!     }
//! }
//!
//! let service = MyService;
//! let _router = Router::new().get(service.hello());
//! ```
//!
//! ## With OpenAPI Support
//!
//! Use `#[craft(endpoint(...))]` for handlers that should be included in
//! OpenAPI documentation:
//!
//! ```
//! use salvo::oapi::OpenApi;
//! use salvo::oapi::extract::QueryParam;
//! use salvo::prelude::*;
//! use salvo_craft::craft;
//!
//! #[derive(Clone, Debug)]
//! pub struct MyService;
//!
//! #[craft]
//! impl MyService {
//!     #[craft(endpoint(tags("users"), status_codes(200, 404)))]
//!     fn get_user(&self, id: QueryParam<i64>) -> String {
//!         format!("user {}", *id)
//!     }
//! }
//!
//! let service = MyService;
//! let router = Router::new().push(Router::with_path("users").get(service.get_user()));
//! let _doc = OpenApi::new("Craft Example", "0.1.0").merge_router(&router);
//! ```
//!
//! # Method Receivers
//!
//! The `#[craft]` macro supports different method receivers:
//!
//! | Receiver | Requirement | Use Case |
//! |----------|-------------|----------|
//! | `&self` | Type must implement `Clone` | Most common, shared state |
//! | `Arc<Self>` | None | Explicit reference counting |
//! | None (static) | None | Stateless handlers |
//!
//! ## Examples
//!
//! ```
//! use std::sync::Arc;
//!
//! use salvo::oapi::extract::QueryParam;
//! use salvo::prelude::*;
//! use salvo_craft::craft;
//!
//! #[derive(Clone, Debug)]
//! pub struct Service {
//!     base: i64,
//! }
//!
//! #[craft]
//! impl Service {
//!     #[craft(handler)]
//!     fn with_ref(&self, value: QueryParam<i64>) -> String {
//!         (self.base + *value).to_string()
//!     }
//!
//!     #[craft(handler)]
//!     fn with_arc(self: Arc<Self>, value: QueryParam<i64>) -> String {
//!         (self.base + *value).to_string()
//!     }
//!
//!     #[craft(handler)]
//!     fn static_handler(value: QueryParam<i64>) -> String {
//!         value.to_string()
//!     }
//! }
//!
//! let service = Arc::new(Service { base: 1 });
//! let _router = Router::new()
//!     .push(Router::with_path("with-ref").get(service.with_ref()))
//!     .push(Router::with_path("with-arc").get(service.with_arc()))
//!     .push(Router::with_path("static").get(Service::static_handler()));
//! ```
//!
//! # Router Integration
//!
//! Craft handlers are used with routers just like regular handlers:
//!
//! ```
//! use salvo::prelude::*;
//! use salvo_craft::craft;
//!
//! #[derive(Clone, Debug)]
//! pub struct UserService;
//!
//! #[craft]
//! impl UserService {
//!     #[craft(handler)]
//!     fn list_users(&self) -> &'static str {
//!         "listing users"
//!     }
//!
//!     #[craft(handler)]
//!     fn health() -> &'static str {
//!         "ok"
//!     }
//! }
//!
//! let service = UserService;
//! let router = Router::new()
//!     .push(Router::with_path("users").get(service.list_users()))
//!     .push(Router::with_path("health").get(UserService::health()));
//! let _ = router;
//! ```
//!
//! For a complete runnable example with OpenAPI integration, see
//! `crates/craft/examples/openapi.rs`.

pub use salvo_craft_macros::*;
