//! Code to convert the Rust-styled field/variant (e.g. `my_field`, `MyType`) to the
//! case of the source (e.g. `my-field`, `MY_FIELD`).

use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;

use proc_macro2::Span;
use syn::Error;

use self::RenameRule::*;

/// The different possible ways to change case of fields in a struct, or variants in an enum.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum RenameRule {
    /// Rename direct children to "lowercase" style.
    LowerCase,
    /// Rename direct children to "UPPERCASE" style.
    UpperCase,
    /// Rename direct children to "PascalCase" style, as typically used for
    /// enum variants.
    PascalCase,
    /// Rename direct children to "camelCase" style.
    CamelCase,
    /// Rename direct children to "snake_case" style, as commonly used for
    /// fields.
    SnakeCase,
    /// Rename direct children to "SCREAMING_SNAKE_CASE" style, as commonly
    /// used for constants.
    ScreamingSnakeCase,
    /// Rename direct children to "kebab-case" style.
    KebabCase,
    /// Rename direct children to "SCREAMING-KEBAB-CASE" style.
    ScreamingKebabCase,
}

pub(crate) const RENAME_RULES: [(&str, RenameRule); 8] = [
    ("lowercase", LowerCase),
    ("UPPERCASE", UpperCase),
    ("PascalCase", PascalCase),
    ("camelCase", CamelCase),
    ("snake_case", SnakeCase),
    ("SCREAMING_SNAKE_CASE", ScreamingSnakeCase),
    ("kebab-case", KebabCase),
    ("SCREAMING-KEBAB-CASE", ScreamingKebabCase),
];

impl Display for RenameRule {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            LowerCase => "lowercase",
            UpperCase => "UPPERCASE",
            PascalCase => "PascalCase",
            CamelCase => "camelCase",
            SnakeCase => "snake_case",
            ScreamingSnakeCase => "SCREAMING_SNAKE_CASE",
            KebabCase => "kebab-case",
            ScreamingKebabCase => "SCREAMING-KEBAB-CASE",
        })
    }
}

impl FromStr for RenameRule {
    type Err = Error;
    fn from_str(rename_all_str: &str) -> Result<Self, Self::Err> {
        RENAME_RULES
            .into_iter()
            .find_map(|(case, rule)| {
                if case == rename_all_str {
                    Some(rule)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                let expected_one_of = RENAME_RULES
                    .into_iter()
                    .map(|(name, _)| format!(r#""{name}""#))
                    .collect::<Vec<_>>()
                    .join(", ");

                Error::new(
                    Span::call_site(),
                    format!(r#"unexpected rename rule, expected one of: {expected_one_of}"#),
                )
            })
    }
}
impl RenameRule {
    /// Apply a renaming rule to an enum variant, returning the version expected in the source.
    pub fn apply_to_variant(self, variant: &str) -> String {
        match self {
            LowerCase => variant.to_ascii_lowercase(),
            UpperCase => variant.to_ascii_uppercase(),
            PascalCase => variant.to_owned(),
            CamelCase => variant[..1].to_ascii_lowercase() + &variant[1..],
            SnakeCase => {
                let mut snake = String::new();
                for (i, ch) in variant.char_indices() {
                    if i > 0 && ch.is_uppercase() {
                        snake.push('_');
                    }
                    snake.push(ch.to_ascii_lowercase());
                }
                snake
            }
            ScreamingSnakeCase => SnakeCase.apply_to_variant(variant).to_ascii_uppercase(),
            KebabCase => SnakeCase.apply_to_variant(variant).replace('_', "-"),
            ScreamingKebabCase => ScreamingSnakeCase
                .apply_to_variant(variant)
                .replace('_', "-"),
        }
    }

    /// Apply a renaming rule to a struct field, returning the version expected in the source.
    pub fn apply_to_field(self, field: &str) -> String {
        match self {
            LowerCase | SnakeCase => field.to_owned(),
            UpperCase => field.to_ascii_uppercase(),
            PascalCase => {
                let mut pascal = String::new();
                let mut capitalize = true;
                for ch in field.chars() {
                    if ch == '_' {
                        capitalize = true;
                    } else if capitalize {
                        pascal.push(ch.to_ascii_uppercase());
                        capitalize = false;
                    } else {
                        pascal.push(ch);
                    }
                }
                pascal
            }
            CamelCase => {
                let pascal = PascalCase.apply_to_field(field);
                pascal[..1].to_ascii_lowercase() + &pascal[1..]
            }
            ScreamingSnakeCase => field.to_ascii_uppercase(),
            KebabCase => field.replace('_', "-"),
            ScreamingKebabCase => ScreamingSnakeCase.apply_to_field(field).replace('_', "-"),
        }
    }
}

#[test]
fn rename_variants() {
    for &(original, lower, upper, camel, snake, screaming, kebab, screaming_kebab) in &[
        (
            "Outcome", "outcome", "OUTCOME", "outcome", "outcome", "OUTCOME", "outcome", "OUTCOME",
        ),
        (
            "VeryTasty",
            "verytasty",
            "VERYTASTY",
            "veryTasty",
            "very_tasty",
            "VERY_TASTY",
            "very-tasty",
            "VERY-TASTY",
        ),
        ("A", "a", "A", "a", "a", "A", "a", "A"),
        ("Z42", "z42", "Z42", "z42", "z42", "Z42", "z42", "Z42"),
    ] {
        assert_eq!(LowerCase.apply_to_variant(original), lower);
        assert_eq!(UpperCase.apply_to_variant(original), upper);
        assert_eq!(PascalCase.apply_to_variant(original), original);
        assert_eq!(CamelCase.apply_to_variant(original), camel);
        assert_eq!(SnakeCase.apply_to_variant(original), snake);
        assert_eq!(ScreamingSnakeCase.apply_to_variant(original), screaming);
        assert_eq!(KebabCase.apply_to_variant(original), kebab);
        assert_eq!(
            ScreamingKebabCase.apply_to_variant(original),
            screaming_kebab
        );
    }
}

#[test]
fn rename_fields() {
    for &(original, upper, pascal, camel, screaming, kebab, screaming_kebab) in &[
        (
            "outcome", "OUTCOME", "Outcome", "outcome", "OUTCOME", "outcome", "OUTCOME",
        ),
        (
            "very_tasty",
            "VERY_TASTY",
            "VeryTasty",
            "veryTasty",
            "VERY_TASTY",
            "very-tasty",
            "VERY-TASTY",
        ),
        ("a", "A", "A", "a", "A", "a", "A"),
        ("z42", "Z42", "Z42", "z42", "Z42", "z42", "Z42"),
    ] {
        assert_eq!(UpperCase.apply_to_field(original), upper);
        assert_eq!(PascalCase.apply_to_field(original), pascal);
        assert_eq!(CamelCase.apply_to_field(original), camel);
        assert_eq!(SnakeCase.apply_to_field(original), original);
        assert_eq!(ScreamingSnakeCase.apply_to_field(original), screaming);
        assert_eq!(KebabCase.apply_to_field(original), kebab);
        assert_eq!(ScreamingKebabCase.apply_to_field(original), screaming_kebab);
    }
}
