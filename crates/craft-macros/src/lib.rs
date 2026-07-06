#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Procedural macros for building Salvo handlers from service methods.
//!
//! This crate is normally used through `salvo_craft::craft`. The macro keeps
//! related handlers inside an `impl` block and generates small callable handler
//! values for the methods marked with `#[craft(handler)]` or
//! `#[craft(endpoint(...))]`.

mod craft;
mod utils;

use proc_macro::TokenStream;
use syn::{Item, parse_macro_input};

/// Converts selected methods in an `impl` block into Salvo handlers.
///
/// Apply `#[craft]` to an `impl` block, then mark individual methods with:
///
/// - `#[craft(handler)]` for a regular Salvo handler.
/// - `#[craft(endpoint(...))]` for a Salvo OpenAPI endpoint handler. The arguments are forwarded to
///   `salvo_oapi::endpoint`.
///
/// Each marked method is rewritten into a handler factory with the same method
/// name. Calling that method returns a handler value suitable for
/// `Router::get`, `Router::post`, and other routing methods; the original
/// method body is moved into the generated handler implementation.
///
/// Supported receivers:
///
/// - `&self`: clones the containing value into the generated handler, so the type must implement
///   `Clone`.
/// - `self: Arc<Self>`: reuses the existing `Arc`.
/// - no receiver: generates a static handler constructor.
///
/// Handler parameters and return values follow the same rules as Salvo's
/// `#[handler]` macro.
///
/// ## Example
/// ```
/// use salvo::oapi::extract::*;
/// use salvo::prelude::*;
/// use salvo_craft_macros::craft;
///
/// #[derive(Clone)]
/// pub struct Service {
///     state: i64,
/// }
///
/// #[craft]
/// impl Service {
///     fn new(state: i64) -> Self {
///         Self { state }
///     }
///     /// doc line 1
///     /// doc line 2
///     #[salvo_craft_macros::craft(handler)]
///     fn add1(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
///         (self.state + *left + *right).to_string()
///     }
///     /// doc line 3
///     /// doc line 4
///     #[craft(handler)]
///     pub(crate) fn add2(
///         self: ::std::sync::Arc<Self>,
///         left: QueryParam<i64>,
///         right: QueryParam<i64>,
///     ) -> String {
///         (self.state + *left + *right).to_string()
///     }
///     /// doc line 5
///     /// doc line 6
///     #[craft(handler)]
///     pub fn add3(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
///         (*left + *right).to_string()
///     }
/// }
/// ```
/// Note: `#[craft(handler)]` can be replaced with `#[craft(endpoint(...))]` for more configuration
/// options.
///
/// When using `&self` as the method receiver, the containing type must implement the `Clone` trait.
#[proc_macro_attribute]
pub fn craft(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match craft::generate(item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
