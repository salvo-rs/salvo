extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use proc_quote::quote;
use syn::Ident;
use syn::ReturnType;

/// ```fn_handler``` is a pro macro to help create ```Handler``` from function easily.
#[proc_macro_attribute]
pub fn fn_handler(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::ItemFn);
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &mut input.sig;
    let body = &input.block;
    let name = &sig.ident;

    let salvo = match crate_name("salvo_core").or_else(|_| crate_name("salvo")) {
        Ok(salvo) => match salvo {
            FoundCrate::Itself => Ident::new("crate", Span::call_site()),
            FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
        },
        Err(_) => Ident::new("crate", Span::call_site()),
    };

    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
            .to_compile_error()
            .into();
    }

    match sig.inputs.len() {
        0 => {
            let ts: TokenStream = quote! {_req: &mut #salvo::Request}.into();
            sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
            let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
            sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
            let ts: TokenStream = quote! {_res: &mut #salvo::Response}.into();
            sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
        }
        1 => match parse_input_type(sig.inputs.first().unwrap()) {
            InputType::Response => {
                let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
                sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
                let ts: TokenStream = quote! {_req: &mut #salvo::Request}.into();
                sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
            }
            InputType::Request => {
                let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
                sig.inputs.insert(1, syn::parse_macro_input!(ts as syn::FnArg));
                let ts: TokenStream = quote! {_res: &mut #salvo::Response}.into();
                sig.inputs.insert(2, syn::parse_macro_input!(ts as syn::FnArg));
            }
            InputType::Depot => {
                let ts: TokenStream = quote! {_res: &mut #salvo::Request}.into();
                sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
                let ts: TokenStream = quote! {_req: &mut #salvo::Response}.into();
                sig.inputs.insert(2, syn::parse_macro_input!(ts as syn::FnArg));
            }
            InputType::UnKnow => {
                return syn::Error::new_spanned(&sig.inputs, "The inputs parameters must be Request, Depot or Response")
                    .to_compile_error()
                    .into()
            }
            InputType::NoReferenceArg => {
                return syn::Error::new_spanned(
                    &sig.inputs,
                    "The inputs parameters must be mutable reference Request, Depot or Response",
                )
                .to_compile_error()
                .into()
            }
        },
        2 => {
            let mut input_types = [InputType::Request, InputType::Request];
            for (index, arg) in sig.inputs.iter().enumerate() {
                input_types[index] = parse_input_type(arg);
            }

            match input_types {
                [InputType::Request, InputType::Response] => {
                    let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
                    sig.inputs.insert(1, syn::parse_macro_input!(ts as syn::FnArg));
                }
                [InputType::Request, InputType::Depot] => {
                    let ts: TokenStream = quote! {_res: &mut #salvo::Response}.into();
                    sig.inputs.insert(2, syn::parse_macro_input!(ts as syn::FnArg));
                }
                [InputType::Depot, InputType::Response] => {
                    let ts: TokenStream = quote! {_req: &mut #salvo::Request}.into();
                    sig.inputs.insert(0, syn::parse_macro_input!(ts as syn::FnArg));
                }
                _ => {
                    return syn::Error::new_spanned(
                        &sig.inputs,
                        "The order of the inputs parameters must be Request, Depot, Response",
                    )
                    .to_compile_error()
                    .into()
                }
            }
        }
        3 => {
            let mut input_types = [InputType::Request, InputType::Request, InputType::Request];
            for (index, arg) in sig.inputs.iter().enumerate() {
                input_types[index] = parse_input_type(arg);
            }

            match input_types {
                [InputType::Request, InputType::Depot, InputType::Response] => (),
                _ => {
                    return syn::Error::new_spanned(
                        &sig.inputs,
                        "The order of the inputs parameters must be Request, Depot, Response",
                    )
                    .to_compile_error()
                    .into()
                }
            }
        }
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
        ReturnType::Default => {
            (quote! {
                #sdef
                #[async_trait]
                impl #salvo::Handler for #name {
                    async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response) {
                        Self::#name(req, depot, res).await
                    }
                }
            })
            .into()
        }
        ReturnType::Type(_, _) => (quote! {
            #sdef
            #[async_trait]
            impl #salvo::Handler for #name {
                async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response) {
                    Self::#name(req, depot, res).await.write(req, depot, res).await;
                }
            }
        })
        .into(),
    }
}

enum InputType {
    Request,
    Depot,
    Response,
    UnKnow,
    NoReferenceArg,
}

fn parse_input_type(input: &syn::FnArg) -> InputType {
    if let syn::FnArg::Typed(p) = input {
        if let syn::Type::Reference(ty) = &*p.ty {
            if let syn::Type::Path(nty) = &*ty.elem {
                // the last ident for path type is the real type
                // such as:
                // `::std::vec::Vec` is `Vec`
                // `Vec` is `Vec`
                let ident = &nty.path.segments.last().unwrap().ident;
                if ident == "Request" {
                    InputType::Request
                } else if ident == "Response" {
                    InputType::Response
                } else if ident == "Depot" {
                    InputType::Depot
                } else {
                    InputType::UnKnow
                }
            } else {
                InputType::UnKnow
            }
        } else {
            // like owned type or other type
            InputType::NoReferenceArg
        }
    } else {
        // like self on fn
        InputType::UnKnow
    }
}
