//! Procedural macros for the Salvo serverless runtimes.
//!
//! This crate is normally used through the re-exports from `salvo` or
//! `salvo-serverless`. It provides the `#[service]` attribute that turns a
//! zero-argument function into a serverless entry point.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(clippy::unwrap_used))]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{ExprPath, Ident, ItemFn, Token, parse_macro_input};

/// Arguments accepted by the [`service`] attribute.
struct ServiceArgs {
    /// Path to a zero-argument initializer called once before the runtime starts.
    start: Option<ExprPath>,
    /// Accepted for forward compatibility; it currently does not change output.
    respond_with_errors: bool,
}

impl Parse for ServiceArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut start = None;
        let mut respond_with_errors = false;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "start" => {
                    input.parse::<Token![=]>()?;
                    start = Some(input.parse::<ExprPath>()?);
                }
                "respond_with_errors" => {
                    if respond_with_errors {
                        return Err(syn::Error::new(
                            key.span(),
                            "duplicate `respond_with_errors` argument",
                        ));
                    }
                    respond_with_errors = true;
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown service argument `{other}`"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            start,
            respond_with_errors,
        })
    }
}

/// Marks a function as the entry point of a serverless service.
///
/// The attributed function must take no arguments. It is renamed to
/// `<name>_service` and keeps its original body. Depending on the enabled
/// runtime features, the macro also generates:
///
/// - with `rt_lambda`, a `main` function that builds a `LambdaServer` from the return value and
///   starts the Lambda HTTP runtime in streaming mode via `run_with_streaming_response`
///   (non-wasm targets only);
/// - with `rt_worker`, a `_salvo_worker_entry` module exposing `#[worker::event]` `start` and
///   `fetch` handlers backed by a `WorkerServer` (wasm targets only).
///
/// Both features can be enabled at once; each entry point is gated by its
/// target architecture so only the matching one is compiled.
///
/// # Arguments
///
/// - `start = <path>` calls the given zero-argument path once on a warm start, before the runtime
///   begins polling for events. The value must be a path expression such as `start = crate::init`
///   or `start = init`.
/// - `respond_with_errors` is accepted and validated, but currently does not change the generated
///   code. Error responses are produced by Salvo's catcher instead.
///
/// # Cloudflare Workers dependency
///
/// The generated Workers entry uses `#[worker::event]`, whose expansion contains
/// hard-coded `::worker::` paths. A crate that uses this entry point must list
/// `worker` as a direct dependency (the standard workers-rs setup); Salvo cannot
/// redirect those macro-internal paths.
///
/// # Example
///
/// ```rust,ignore
/// use salvo::prelude::*;
///
/// #[service(start = init)]
/// fn build() -> Router {
///     Router::new().push(Router::with_path("hello").get(|| async { "hello" }))
/// }
///
/// fn init() {
///     // one-time initialization
/// }
/// ```
///
/// Enable at least one runtime feature of `salvo` (`rt_lambda` and/or
/// `rt_worker`), otherwise expansion emits a `compile_error!`.
#[proc_macro_attribute]
pub fn service(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ServiceArgs);
    let input = parse_macro_input!(item as ItemFn);
    generate(args, input).into()
}

fn generate(args: ServiceArgs, mut input: ItemFn) -> TokenStream2 {
    let original = input.sig.ident.clone();
    let name = Ident::new(&format!("{original}_service"), original.span());
    input.sig.ident = name.clone();

    let has_start = args.start.is_some();
    let start = args.start.map(|path| quote! { #path(); });
    // Parsed and validated, but error responses are handled by Salvo's catcher.
    let _ = args.respond_with_errors;

    let worker_start = if has_start {
        quote! {
            #[worker::event(start)]
            fn start() {
                #start
            }
        }
    } else {
        quote! {}
    };

    let mut runtime = TokenStream2::new();
    let mut enabled = false;

    if cfg!(feature = "rt_lambda") {
        enabled = true;
        runtime.extend(quote! {
            #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
            #[::salvo::serverless::tokio::main(crate = "::salvo::serverless::tokio")]
            async fn main() -> Result<(), ::salvo::serverless::lambda_http::Error> {
                #start
                let server: ::salvo::serverless::lambda::LambdaServer = #name().into();
                ::salvo::serverless::lambda_http::run_with_streaming_response(
                    ::salvo::serverless::lambda_http::service_fn(move |req| {
                        let server = server.clone();
                        async move { server.handle_streaming(req).await }
                    }),
                )
                .await
            }
        });
    }

    if cfg!(feature = "rt_worker") {
        enabled = true;
        runtime.extend(quote! {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            mod _salvo_worker_entry {
                use super::*;
                use ::salvo::serverless::worker::{
                    self, Context, Env, HttpRequest, HttpResponse, WorkerServer,
                };
                use std::sync::OnceLock;

                static SERVER: OnceLock<WorkerServer> = OnceLock::new();

                #worker_start

                #[worker::event(fetch)]
                async fn fetch(req: HttpRequest, env: Env, ctx: Context) -> worker::Result<HttpResponse> {
                    let server = SERVER.get_or_init(|| #name().into());
                    server.handle(req, env, ctx).await
                }
            }
        });
    }

    if !enabled {
        runtime.extend(quote! {
            compile_error!("enable `rt_lambda` or `rt_worker` on `salvo-serverless` to use #[service]");
        });
    }

    quote! {
        #input
        #runtime
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse2;

    use super::*;

    fn expand(attr: TokenStream2, item: TokenStream2, compact: bool) -> String {
        let args: ServiceArgs = parse2(attr).unwrap();
        let input: ItemFn = parse2(item).unwrap();
        let tokens = generate(args, input).to_string();
        if compact {
            tokens.chars().filter(|c| !c.is_whitespace()).collect()
        } else {
            tokens
        }
    }

    #[test]
    fn service_renames_the_original_function() {
        let expanded = expand(
            quote! { start = crate::init, respond_with_errors },
            quote! {
                fn build() -> ::salvo::Router {
                    ::salvo::Router::new()
                }
            },
            true,
        );
        assert!(expanded.contains("fnbuild_service()"), "{expanded}");
        assert!(expanded.contains("::salvo::Router::new()"), "{expanded}");
    }

    #[cfg(feature = "rt_lambda")]
    #[test]
    fn service_generates_lambda_main() {
        let expanded = expand(
            quote! { start = crate::init },
            quote! {
                fn build() -> ::salvo::Router {
                    ::salvo::Router::new()
                }
            },
            true,
        );
        assert!(
            expanded.contains("::salvo::serverless::lambda::LambdaServer"),
            "{expanded}"
        );
        assert!(
            expanded.contains("::salvo::serverless::lambda_http::run_with_streaming_response"),
            "{expanded}"
        );
        assert!(
            expanded.contains("handle_streaming(req)"),
            "{expanded}"
        );
        assert!(expanded.contains("crate::init()"), "{expanded}");
        assert!(
            expanded.contains("#[cfg(not(any(target_arch=\"wasm32\",target_arch=\"wasm64\")))]"),
            "{expanded}"
        );
    }

    #[cfg(feature = "rt_worker")]
    #[test]
    fn service_generates_worker_entry() {
        let expanded = expand(
            quote! { start = crate::init },
            quote! {
                fn build() -> ::salvo::Router {
                    ::salvo::Router::new()
                }
            },
            true,
        );
        assert!(expanded.contains("_salvo_worker_entry"), "{expanded}");
        assert!(expanded.contains("#[worker::event(fetch)]"), "{expanded}");
        assert!(expanded.contains("HttpRequest"), "{expanded}");
        assert!(expanded.contains("OnceLock"), "{expanded}");
        assert!(expanded.contains("WorkerServer"), "{expanded}");
        assert!(expanded.contains("#[worker::event(start)]"), "{expanded}");
        assert!(expanded.contains("crate::init()"), "{expanded}");
        assert!(
            expanded.contains("#[cfg(any(target_arch=\"wasm32\",target_arch=\"wasm64\"))]"),
            "{expanded}"
        );
    }
}