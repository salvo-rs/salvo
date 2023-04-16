use std::borrow::Cow;
use std::io;

use proc_macro2::{Group, Ident, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::parse::Parse;
use syn::token::Comma;
use syn::{parenthesized, Error, LitStr, Token};

use crate::parse_utils;

#[derive(Clone,Debug)]
pub(super) enum Str {
    String(String),
    IncludeStr(TokenStream2),
}

impl Parse for Str {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(Self::String(input.parse::<LitStr>()?.value()))
        } else {
            let include_str = input.parse::<Ident>()?;
            let bang = input.parse::<Option<Token![!]>>()?;
            if include_str != "include_str" || bang.is_none() {
                return Err(Error::new(
                    include_str.span(),
                    "unexpected token, expected either literal string or include_str!(...)",
                ));
            }
            Ok(Self::IncludeStr(input.parse::<Group>()?.stream()))
        }
    }
}

impl ToTokens for Str {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            Self::String(str) => str.to_tokens(tokens),
            Self::IncludeStr(include_str) => tokens.extend(quote! { include_str!(#include_str) }),
        }
    }
}

#[derive(Default, Clone,Debug)]
pub(super) struct Info<'i> {
    title: Option<String>,
    version: Option<String>,
    description: Option<Str>,
    license: Option<License<'i>>,
    contact: Option<Contact<'i>>,
}

impl Info<'_> {
    /// Construct new [`Info`] from _`cargo`_ env variables such as
    /// * `CARGO_PGK_NAME`
    /// * `CARGO_PGK_VERSION`
    /// * `CARGO_PGK_DESCRIPTION`
    /// * `CARGO_PGK_AUTHORS`
    /// * `CARGO_PGK_LICENSE`
    fn from_env() -> Self {
        let name = std::env::var("CARGO_PKG_NAME").ok();
        let version = std::env::var("CARGO_PKG_VERSION").ok();
        let description = std::env::var("CARGO_PKG_DESCRIPTION").ok().map(Str::String);
        let contact = std::env::var("CARGO_PKG_AUTHORS")
            .ok()
            .and_then(|authors| Contact::try_from(authors).ok())
            .and_then(|contact| {
                if contact.name.is_none() && contact.email.is_none() && contact.url.is_none() {
                    None
                } else {
                    Some(contact)
                }
            });
        let license = std::env::var("CARGO_PKG_LICENSE").ok().map(License::from);

        Info {
            title: name,
            version,
            description,
            contact,
            license,
        }
    }
}

impl Parse for Info<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut info = Info::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>()?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "title" => {
                    info.title =
                        Some(parse_utils::parse_next(input, || input.parse::<LitStr>())?.value())
                }
                "version" => {
                    info.version =
                        Some(parse_utils::parse_next(input, || input.parse::<LitStr>())?.value())
                }
                "description" => {
                    info.description =
                        Some(parse_utils::parse_next(input, || input.parse::<Str>())?)
                }
                "license" => {
                    let license_stream;
                    parenthesized!(license_stream in input);
                    info.license = Some(license_stream.parse()?)
                }
                "contact" => {
                    let contact_stream;
                    parenthesized!(contact_stream in input);
                    info.contact = Some(contact_stream.parse()?)
                }
                _ => {
                    return Err(Error::new(ident.span(), format!("unexpected attribute: {attribute_name}, expected one of: title, version, description, license, contact")));
                }
            }
            if !input.is_empty() {
                input.parse::<Comma>()?;
            }
        }

        Ok(info)
    }
}

impl ToTokens for Info<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        let title = self.title.as_ref().map(|title| quote! { .title(#title) });
        let version = self
            .version
            .as_ref()
            .map(|version| quote! { .version(#version) });
        let description = self
            .description
            .as_ref()
            .map(|description| quote! { .description(Some(#description)) });
        let license = self
            .license
            .as_ref()
            .map(|license| quote! { .license(Some(#license)) });
        let contact = self
            .contact
            .as_ref()
            .map(|contact| quote! { .contact(Some(#contact)) });

        tokens.extend(quote! {
            #root::oapi::openapi::InfoBuilder::new()
                #title
                #version
                #description
                #license
                #contact
        })
    }
}

#[derive(Default, Clone,Debug)]
pub(super) struct License<'l> {
    name: Cow<'l, str>,
    url: Option<Cow<'l, str>>,
}

impl Parse for License<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut license = License::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>()?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "name" => {
                    license.name = Cow::Owned(
                        parse_utils::parse_next(input, || input.parse::<LitStr>())?.value(),
                    )
                }
                "url" => {
                    license.url = Some(Cow::Owned(
                        parse_utils::parse_next(input, || input.parse::<LitStr>())?.value(),
                    ))
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {attribute_name}, expected one of: name, url"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Comma>()?;
            }
        }

        Ok(license)
    }
}

impl ToTokens for License<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        let name = &self.name;
        let url = self.url.as_ref().map(|url| quote! { .url(Some(#url))});

        tokens.extend(quote! {
            #root::oapi::openapi::info::LicenseBuilder::new()
                .name(#name)
                #url
                .build()
        })
    }
}

impl From<String> for License<'_> {
    fn from(string: String) -> Self {
        License {
            name: Cow::Owned(string),
            ..Default::default()
        }
    }
}

#[derive(Default, Clone,Debug)]
pub(super) struct Contact<'c> {
    name: Option<Cow<'c, str>>,
    email: Option<Cow<'c, str>>,
    url: Option<Cow<'c, str>>,
}

impl Parse for Contact<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut contact = Contact::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>()?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "name" => {
                    contact.name = Some(Cow::Owned(
                        parse_utils::parse_next(input, || input.parse::<LitStr>())?.value(),
                    ))
                }
                "email" => {
                    contact.email = Some(Cow::Owned(
                        parse_utils::parse_next(input, || input.parse::<LitStr>())?.value(),
                    ))
                }
                "url" => {
                    contact.url = Some(Cow::Owned(
                        parse_utils::parse_next(input, || input.parse::<LitStr>())?.value(),
                    ))
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!("unexpected attribute: {attribute_name}, expected one of: name, email, url"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Comma>()?;
            }
        }

        Ok(contact)
    }
}

impl ToTokens for Contact<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        let name = self.name.as_ref().map(|name| quote! { .name(Some(#name)) });
        let email = self
            .email
            .as_ref()
            .map(|email| quote! { .email(Some(#email)) });
        let url = self.url.as_ref().map(|url| quote! { .url(Some(#url)) });

        tokens.extend(quote! {
            #root::oapi::openapi::info::ContactBuilder::new()
                #name
                #email
                #url
                .build()
        })
    }
}

impl TryFrom<String> for Contact<'_> {
    type Error = io::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if let Some((name, email)) = get_parsed_author(value.split(':').next()) {
            let non_empty = |value: &str| -> Option<Cow<'static, str>> {
                if !value.is_empty() {
                    Some(Cow::Owned(value.to_string()))
                } else {
                    None
                }
            };
            Ok(Contact {
                name: non_empty(name),
                email: non_empty(email),
                ..Default::default()
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("invalid contact: {value}"),
            ))
        }
    }
}

pub(super) fn impl_info(parsed: Option<Info>) -> Info {
    let mut info = Info::from_env();

    if let Some(parsed) = parsed {
        if parsed.title.is_some() {
            info.title = parsed.title;
        }

        if parsed.description.is_some() {
            info.description = parsed.description;
        }

        if parsed.license.is_some() {
            info.license = parsed.license;
        }

        if parsed.contact.is_some() {
            info.contact = parsed.contact;
        }

        if parsed.version.is_some() {
            info.version = parsed.version;
        }
    }

    info
}

fn get_parsed_author(author: Option<&str>) -> Option<(&str, &str)> {
    author.map(|author| {
        let mut author_iter = author.split('<');

        let name = author_iter.next().unwrap_or_default();
        let mut email = author_iter.next().unwrap_or_default();
        if !email.is_empty() {
            email = &email[..email.len() - 1];
        }

        (name.trim_end(), email)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_author_with_email_success() {
        let author = "Tessu Tester <tessu@steps.com>";

        if let Some((name, email)) = get_parsed_author(Some(author)) {
            assert_eq!(
                name, "Tessu Tester",
                "expected name {} != {}",
                "Tessu Tester", name
            );
            assert_eq!(
                email, "tessu@steps.com",
                "expected email {} != {}",
                "tessu@steps.com", email
            );
        } else {
            panic!("Expected Some(Tessu Tester, tessu@steps.com), but was none")
        }
    }

    #[test]
    fn parse_author_only_name() {
        let author = "Tessu Tester";

        if let Some((name, email)) = get_parsed_author(Some(author)) {
            assert_eq!(
                name, "Tessu Tester",
                "expected name {} != {}",
                "Tessu Tester", name
            );
            assert_eq!(email, "", "expected email {} != {}", "", email);
        } else {
            panic!("Expected Some(Tessu Tester, ), but was none")
        }
    }

    #[test]
    fn contact_from_only_name() {
        let author = "Suzy Lin";
        let contanct = Contact::try_from(author.to_string()).unwrap();

        assert!(contanct.name.is_some(), "Suzy should have name");
        assert!(contanct.email.is_none(), "Suzy should not have email");
    }

    #[test]
    fn contact_from_name_and_email() {
        let author = "Suzy Lin <suzy@lin.com>";
        let contanct = Contact::try_from(author.to_string()).unwrap();

        assert!(contanct.name.is_some(), "Suzy should have name");
        assert!(contanct.email.is_some(), "Suzy should have email");
    }

    #[test]
    fn contact_from_empty() {
        let author = "";
        let contact = Contact::try_from(author.to_string()).unwrap();

        assert!(contact.name.is_none(), "Contat name should be empty");
        assert!(contact.email.is_none(), "Contat email should be empty");
    }
}
