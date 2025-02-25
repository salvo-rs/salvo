//! [`Salvo`](https://github.com/salvo-rs/salvo) `Handler` modular craft macros.

mod craft;
mod utils;

use proc_macro::TokenStream;
use syn::{Item, parse_macro_input};

/// `#[craft]` is an attribute macro that converts methods in an `impl` block into [`Salvo`'s `Handler`](https://github.com/salvo-rs/salvo) implementations.
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
///     pub(crate) fn add2( self: ::std::sync::Arc<Self>, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
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
/// Note: `#[craft(handler)]` can be replaced with `#[craft(endpoint(...))]` for more configuration options.
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
