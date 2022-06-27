
#[derive(FromDeriveInput)]
#[darling(attributes(salvo::extract))]
struct ExtractibleArgs {
    ident: Ident,
    generics: Generics,
    attrs: Vec<Attribute>,
    data: Data<Ignored, ExtractibleField>,
    default_from: Option<Ident>,
}


#[proc_macro_derive(Extractible, attributes(salvo::extract))]
pub fn derive_extractible(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DeriveInput);
 
    impl #impl_generics #crate_name::extract::Extractible for #ident #ty_generics #where_clause {
        fn metadata() -> #crate_name::extract::Metadata {
           
        }
    }
}
