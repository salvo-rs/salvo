use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::token::{Bracket, Comma};
use syn::{bracketed, parenthesized, Result, Token};

use crate::operation::example::Example;
use crate::{parse_utils, AnyValue};

use super::Header;

#[inline]
pub(super) fn description(input: ParseStream) -> Result<parse_utils::Value> {
    parse_utils::parse_next_lit_str_or_expr(input)
}

#[inline]
pub(super) fn content_type(input: ParseStream) -> Result<Vec<parse_utils::Value>> {
    parse_utils::parse_next(input, || {
        let look_content_type = input.lookahead1();
        if look_content_type.peek(Bracket) {
            let content_types;
            bracketed!(content_types in input);
            Ok(
                Punctuated::<parse_utils::Value, Comma>::parse_terminated(&content_types)?
                    .into_iter()
                    .collect(),
            )
        } else {
            Ok(vec![input.parse::<parse_utils::Value>()?])
        }
    })
}

#[inline]
pub(super) fn headers(input: ParseStream) -> Result<Vec<Header>> {
    let headers;
    parenthesized!(headers in input);

    parse_utils::parse_groups(&headers)
}

#[inline]
pub(super) fn example(input: ParseStream) -> Result<AnyValue> {
    parse_utils::parse_next(input, || AnyValue::parse_lit_str_or_json(input))
}

#[inline]
pub(super) fn examples(input: ParseStream) -> Result<Punctuated<Example, Token![,]>> {
    parse_utils::parse_punctuated_within_parenthesis(input)
}
