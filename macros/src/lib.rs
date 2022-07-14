//! The macros lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub, unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, AttributeArgs, DeriveInput, ItemFn, ItemImpl, Meta, NestedMeta, Token};

mod extract;
mod handler;
mod shared;

pub(crate) enum Item {
    Fn(ItemFn),
    Impl(ItemImpl),
}
impl Parse for Item {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(Token![impl]) {
            input.parse().map(Item::Impl)
        } else {
            input
                .parse()
                .map(Item::Fn)
                .map_err(|_| syn::Error::new(Span::call_site(), "#[handler] must added to `impl` or `fn`"))
        }
    }
}
/// `handler` is a pro macro to help create `Handler` from function or impl block easily.
///
/// `Handler` is a trait, if `#[handler]` applied to `fn`,  `fn` will converted to a struct, and then implement `Handler`.
///
/// ```ignore
/// #[async_trait]
/// pub trait Handler: Send + Sync + 'static {
///     async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
/// }
/// ```
///
/// After use `handler`, you don't need to care arguments' order, omit unused arguments:
///
/// ```ignore
/// #[handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
/// ```
#[proc_macro_attribute]
pub fn handler(args: TokenStream, input: TokenStream) -> TokenStream {
    let args: AttributeArgs = parse_macro_input!(args as AttributeArgs);
    let mut internal = false;
    for arg in args {
        if matches!(arg,NestedMeta::Meta(Meta::Path(p)) if p.is_ident("internal")) {
            internal = true;
            break;
        }
    }
    let item = parse_macro_input!(input as Item);
    match handler::generate(internal, item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `handler` is a pro macro to help create `Handler` from function easily.
///
/// Note: This is a deprecated version, please use `handler` instead, this will be removed in the future.
#[deprecated(
    since = "0.27.0",
    note = "please use `handler` instead, this will be removed in the future"
)]
#[proc_macro_attribute]
pub fn fn_handler(args: TokenStream, input: TokenStream) -> TokenStream {
    handler(args, input)
}

/// Generate code for extractible type.
#[proc_macro_derive(Extractible, attributes(extract))]
pub fn derive_extractible(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DeriveInput);
    match extract::generate(args) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
