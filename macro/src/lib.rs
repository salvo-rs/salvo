extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::ReturnType;

#[proc_macro_attribute]
pub fn fn_handler(_: TokenStream, input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemFn);
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let body = &input.block;
    let name = &sig.ident;

    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
            .to_compile_error()
            .into();
    }

    match sig.output {
        ReturnType::Default => {
            (quote! {
                #[allow(non_camel_case_types)]
                #vis struct #name;
                impl #name {
                    #(#attrs)*
                    #sig {
                        #body
                    }
                }
                #[async_trait]
                impl salvo::Handler for #name {
                    async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
                        Self::#name(sconf, req, depot, resp).await
                    }
                }
            }).into()
        },
        ReturnType::Type(_, _) => {
            (quote! {
                #[allow(non_camel_case_types)]
                #vis struct #name;
                impl #name {
                    #(#attrs)*
                    #sig {
                        #body
                    }
                }
                #[async_trait]
                impl salvo::Handler for #name {
                    async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
                        match Self::#name(sconf, req, depot, resp).await {
                            Ok(content) => ::salvo::Content::apply(content, resp),
                            Err(err) => {
                                resp.set_status_code(::salvo::HandleError::http_code(&err));
                                let format = ::salvo::http::guess_accept_mime(req, None);
                                let (format, data) = ::salvo::HandleError::http_body(&err, &format);
                                resp.headers_mut().insert(::salvo::http::header::CONTENT_TYPE, format.to_string().parse().unwrap());
                                resp.write_body(data);
                            },
                        }
                    }
                }
            }).into()
        }
    }
}