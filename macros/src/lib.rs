extern crate proc_macro;
use proc_macro::TokenStream;
use proc_quote::quote;
use syn::ReturnType;
use syn::Ident;
use proc_macro2::Span;
use proc_macro_crate::crate_name;

#[proc_macro_attribute]
pub fn fn_handler(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::ItemFn);
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &mut input.sig;
    let body = &input.block;
    let name = &sig.ident;

    let salvo = crate_name("salvo_core").or_else(|_|crate_name("salvo")).unwrap_or("salvo_core".into());
    let salvo = Ident::new(&salvo, Span::call_site());

    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
            .to_compile_error()
            .into();
    }

    match sig.inputs.len() {
        0 => {
            return syn::Error::new_spanned(&sig.inputs, "0 args found in handler")
                .to_compile_error()
                .into()
        }
        1 => {
            let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
            sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
            let ts: TokenStream = quote! {_req: &mut #salvo::Request}.into();
            sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
        }
        2 => {
            let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
            sig.inputs.insert(1, syn::parse_macro_input!(ts as syn::FnArg));
        }
        3 => {}
        _ => {
            return syn::Error::new_spanned(&sig.inputs, "too many args in handler")
                .to_compile_error()
                .into()
        }
    }

    let sdef = quote! {
        #[allow(non_camel_case_types)]
        #[derive(Debug)]
        #vis struct #name;
        impl #name {
            #(#attrs)*
            #sig {
                #body
            }
        }
    };

    match sig.output {
        ReturnType::Default => (quote! {
            #sdef
            #[async_trait]
            impl #salvo::Handler for #name {
                async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response) {
                    Self::#name(req, depot, res).await
                }
            }
        })
        .into(),
        ReturnType::Type(_, _) => (quote! {
            #sdef
            #[async_trait]
            impl #salvo::Handler for #name {
                async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response) {
                    match Self::#name(req, depot, res).await {
                        Ok(writer) => #salvo::Writer::write(writer,  req, depot, res).await,
                        Err(err) => #salvo::Writer::write(err,  req, depot, res).await,
                    }
                }
            }
        })
        .into(),
    }
}
