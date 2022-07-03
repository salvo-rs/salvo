use std::str::FromStr;
use std::vec;

use self::RenameRule::*;


/// Source from for a field.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFrom {
    /// The field will extracted from url param.
    Param,
    /// The field will extracted from url query.
    Query,
    /// The field will extracted from http header.
    Header,
    /// The field will extracted from http payload.
    Body,
    /// The field will extracted from request.
    Request,
}

impl FromStr for SourceFrom {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "param" => Ok(Self::Param),
            "query" => Ok(Self::Query),
            "header" => Ok(Self::Header),
            "body" => Ok(Self::Body),
            "request" => Ok(Self::Request),
            _ => Err(crate::Error::Other(format!("invalid source from `{}`", input).into())),
        }
    }
}

/// Rename rule for a field.
#[derive(Copy, Clone, PartialEq, Debug)]
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

impl FromStr for RenameRule {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        for (name, rule) in RENAME_RULES {
            if input == *name {
                return Ok(*rule);
            }
        }
        Err(crate::Error::other(format!("invalid rename rule: {}", input)))
    }
}

static RENAME_RULES: &[(&str, RenameRule)] = &[
    ("lowercase", LowerCase),
    ("UPPERCASE", UpperCase),
    ("PascalCase", PascalCase),
    ("camelCase", CamelCase),
    ("snake_case", SnakeCase),
    ("SCREAMING_SNAKE_CASE", ScreamingSnakeCase),
    ("kebab-case", KebabCase),
    ("SCREAMING-KEBAB-CASE", ScreamingKebabCase),
];
impl RenameRule {
    /// Apply a renaming rule to an variant, returning the version expected in the source.
    pub fn transform(&self, variant: &str) -> String {
        match *self {
            PascalCase => variant.to_owned(),
            LowerCase => variant.to_ascii_lowercase(),
            UpperCase => variant.to_ascii_uppercase(),
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
            ScreamingSnakeCase => SnakeCase.transform(variant).to_ascii_uppercase(),
            KebabCase => SnakeCase.transform(variant).replace('_', "-"),
            ScreamingKebabCase => ScreamingSnakeCase.transform(variant).replace('_', "-"),
        }
    }
}

/// Source format for a source. This format is just means that field format, not the request mime type.
/// For example, the request is posted as form, but if the field is string as json format, it can be parsed as json.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFormat {
    /// MulitMap format. This is the default.
    MultiMap,
    /// Json format.
    Json,
    /// Request format means this field is [`Extractible`] and it will extract from the request.
    Request,
}

impl FromStr for SourceFormat {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "multimap" => Ok(Self::MultiMap),
            "json" => Ok(Self::Json),
            "request" => Ok(Self::Request),
            _ => Err(crate::Error::Other("invalid source format".into())),
        }
    }
}

/// Struct's metadata information.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// The name of this type.
    pub name: &'static str,
    /// Default sources of all fields.
    pub default_sources: Vec<Source>,
    /// Fields of this type.
    pub fields: Vec<Field>,
    /// Rename rule for all fields of this type.
    pub rename_all: Option<RenameRule>,
}

/// Information about struct field.
#[derive(Clone, Debug)]
pub struct Field {
    /// Field name.
    pub name: &'static str,
    /// Field sources.
    pub sources: Vec<Source>,
    /// Field aliaes.
    pub aliases: Vec<&'static str>,
    /// Field rename.
    pub rename: Option<&'static str>,
    /// Field metadata. This is used for nested extractible types.
    pub metadata: Option<&'static Metadata>,
}
impl Field {
    /// Create a new field with the given name and kind.
    pub fn new(name: &'static str) -> Self {
        Self::with_sources(name, vec![])
    }

    /// Create a new field with the given name and kind, and the given sources.
    pub fn with_sources(name: &'static str, sources: Vec<Source>) -> Self {
        Self {
            name,
            sources,
            aliases: vec![],
            rename: None,
            metadata: None,
        }
    }

    /// Set the metadata to the field type.
    pub fn metadata(mut self, metadata: &'static Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Add a source to sources list.
    pub fn add_source(mut self, source: Source) -> Self {
        self.sources.push(source);
        self
    }

    /// Set the aliases list to a new value.
    pub fn set_aliases(mut self, aliases: Vec<&'static str>) -> Self {
        self.aliases = aliases;
        self
    }

    /// Add a alias to aliases list.
    pub fn add_alias(mut self, alias: &'static str) -> Self {
        self.aliases.push(alias);
        self
    }

    /// Set the rename to the given value.
    pub fn rename(mut self, rename: &'static str) -> Self {
        self.rename = Some(rename);
        self
    }
}

/// Request source for extract data.
#[derive(Copy, Clone, Debug)]
pub struct Source {
    /// The source from.
    pub from: SourceFrom,
    /// the origin data format of the field.
    pub format: SourceFormat,
}
impl Source {
    /// Create a new source from a string.
    pub fn new(from: SourceFrom, format: SourceFormat) -> Self {
        Self { from, format }
    }
}

impl Metadata {
    /// Create a new metadata object.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            default_sources: vec![],
            fields: Vec::with_capacity(8),
            rename_all: None,
        }
    }

    /// Set the default sources list to a new value.
    pub fn set_default_sources(mut self, default_sources: Vec<Source>) -> Self {
        self.default_sources = default_sources;
        self
    }

    /// set all fields list to a new value.
    pub fn set_fields(mut self, fields: Vec<Field>) -> Self {
        self.fields = fields;
        self
    }

    /// Add a default source to default sources list.
    pub fn add_default_source(mut self, source: Source) -> Self {
        self.default_sources.push(source);
        self
    }

    /// Add a field to the fields list.
    pub fn add_field(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }

    /// Rule for rename all fields of type.
    pub fn rename_all(mut self, rename_all: RenameRule) -> Self {
        self.rename_all = Some(rename_all);
        self
    }
}
