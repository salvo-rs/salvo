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
//! ```ignore
//! use salvo::prelude::*;
//! use salvo_craft::craft;
//!
//! #[derive(Clone)]
//! pub struct UserService {
//!     db: DatabasePool,
//! }
//!
//! #[craft]
//! impl UserService {
//!     fn new(db: DatabasePool) -> Self {
//!         Self { db }
//!     }
//!
//!     #[craft(handler)]
//!     async fn get_user(&self, id: PathParam<i64>) -> Result<Json<User>, StatusError> {
//!         let user = self.db.get_user(*id).await?;
//!         Ok(Json(user))
//!     }
//!
//!     #[craft(handler)]
//!     async fn list_users(&self) -> Json<Vec<User>> {
//!         Json(self.db.list_users().await)
//!     }
//! }
//! ```
//!
//! # Usage
//!
//! ## Basic Handler
//!
//! Use `#[craft(handler)]` to mark a method as a handler:
//!
//! ```ignore
//! #[craft]
//! impl MyService {
//!     #[craft(handler)]
//!     fn hello(&self) -> &'static str {
//!         "Hello, World!"
//!     }
//! }
//! ```
//!
//! ## With OpenAPI Support
//!
//! Use `#[craft(endpoint(...))]` for handlers that should be included in
//! OpenAPI documentation:
//!
//! ```ignore
//! #[craft]
//! impl MyService {
//!     #[craft(endpoint(tags("users"), status_codes(200, 404)))]
//!     async fn get_user(&self, id: PathParam<i64>) -> Result<Json<User>, StatusError> {
//!         // ...
//!     }
//! }
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
//! ```ignore
//! #[derive(Clone)]
//! pub struct Service { /* ... */ }
//!
//! #[craft]
//! impl Service {
//!     // Uses &self - Service must be Clone
//!     #[craft(handler)]
//!     fn with_ref(&self) -> String { /* ... */ }
//!
//!     // Uses Arc<Self> - explicit shared ownership
//!     #[craft(handler)]
//!     fn with_arc(self: Arc<Self>) -> String { /* ... */ }
//!
//!     // Static method - no self
//!     #[craft(handler)]
//!     fn static_handler() -> String { /* ... */ }
//! }
//! ```
//!
//! # Router Integration
//!
//! Craft handlers are used with routers just like regular handlers:
//!
//! ```ignore
//! let service = UserService::new(db_pool);
//!
//! let router = Router::new()
//!     .push(Router::with_path("users/<id>").get(service.get_user()))
//!     .push(Router::with_path("users").get(service.list_users()));
//! ```

pub use salvo_craft_macros::*;
