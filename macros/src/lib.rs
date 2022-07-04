//! The macros lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub, unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, ItemFn};

mod shared;
mod extract;
mod handler;

/// `fn_handler` is a pro macro to help create `Handler` from function easily.
///
/// `Handler` is a trait, `fn_handler` will convert you `fn` to a struct, and then implement `Handler`.
///
/// ```ignore
/// #[async_trait]
/// pub trait Handler: Send + Sync + 'static {
///     async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
/// }
/// ```
///
/// After use `fn_handler`, you don't need to care arguments' order, omit unused arguments:
///
/// ```ignore
/// #[fn_handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
/// ```
#[proc_macro_attribute]
pub fn fn_handler(args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as ItemFn);
    handler::fn_handler(args, item_fn)
}

// #[proc_macro_attribute]
// pub fn handler(args: TokenStream, input: TokenStream) -> TokenStream {

// }

/// Generate code for extractible type.
#[proc_macro_derive(Extractible, attributes(extract))]
pub fn derive_extractible(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DeriveInput);
    match extract::generate(args) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
