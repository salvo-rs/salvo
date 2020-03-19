extern crate proc_macro;
use proc_macro::TokenStream;
use proc_quote::quote;
use syn::ReturnType;

#[proc_macro_attribute]
pub fn fn_handler(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::ItemFn);
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &mut input.sig;
    let body = &input.block;
    let name = &sig.ident;

    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
            .to_compile_error()
            .into();
    }

    match sig.inputs.len() {
        3 => {
            let ts: TokenStream = quote!{_sconf: ::std::sync::Arc<::salvo::ServerConfig>}.into();
            sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
        },
        4 => {},
        _ => return syn::Error::new_spanned(&sig.inputs, "numbers of fn is not supports").to_compile_error().into(),
    }

    let sdef = quote! {
        #[allow(non_camel_case_types)]
        #vis struct #name;
        impl #name {
            #(#attrs)*
            #sig {
                #body
            }
        }
    };

    match sig.output {
        ReturnType::Default => {
            (quote! {
                #sdef
                #[async_trait]
                impl salvo::Handler for #name {
                    async fn handle(&self, sconf: ::std::sync::Arc<::salvo::ServerConfig>, req: &mut ::salvo::Request, depot: &mut ::salvo::Depot, resp: &mut ::salvo::Response) {
                        Self::#name(sconf, req, depot, resp).await
                    }
                }
            }).into()
        },
        ReturnType::Type(_, _) => {
            (quote! {
                #sdef
                #[async_trait]
                impl salvo::Handler for #name {
                    async fn handle(&self, sconf: ::std::sync::Arc<::salvo::ServerConfig>, req: &mut ::salvo::Request, depot: &mut ::salvo::Depot, resp: &mut ::salvo::Response) {
                        match Self::#name(sconf, req, depot, resp).await {
                            Ok(writer) => ::salvo::Writer::write(writer, sconf, req, depot, resp).await,
                            Err(err) => ::salvo::Writer::write(err, sconf, req, depot, resp).await,
                        }
                    }
                }
            }).into()
        }
    }
}