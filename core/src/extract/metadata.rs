use std::vec;
use std::str::FromStr;

use self::RenameRule::*;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum DataKind {
    Enum,
    Struct,
}
impl FromStr for DataKind {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "enum" => Ok(Self::Enum),
            "struct" => Ok(Self::Struct),
            _ => Err(crate::Error::Other("invalid data kind".into())),
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFrom {
    Param,
    Query,
    Header,
    Body,
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
            _ => Err(crate::Error::Other("invalid source from".into())),
        }
    }
}

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
        Self::from_str(input)
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
    pub fn from_str(rename_all_str: &str) -> Result<Self, crate::Error> {
        for (name, rule) in RENAME_RULES {
            if rename_all_str == *name {
                return Ok(*rule);
            }
        }
        Err(crate::Error::other(format!("invalid rename rule: {}", rename_all_str)))
    }

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
            ScreamingKebabCase => ScreamingSnakeCase
                .transform(variant)
                .replace('_', "-"),
        }
    }
}



#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFormat {
    MultiMap,
    Json,
    Request,
}

impl FromStr for SourceFormat {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "multimap" => Ok(Self::MultiMap),
            "json" => Ok(Self::Json),
            _ => Err(crate::Error::Other("invalid source format".into())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub name: &'static str,
    pub kind: DataKind,
    pub default_sources: Vec<Source>,
    pub fields: Vec<Field>,
    pub rename_all: Option<RenameRule>,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: &'static str,
    pub kind: DataKind,
    pub sources: Vec<Source>,
    pub aliases: Vec<&'static str>,
    pub rename: Option<&'static str>,
}
impl Field {
    pub fn new(name: &'static str, kind: DataKind) -> Self {
        Self::with_sources(name, kind, vec![])
    }
    pub fn with_sources(name: &'static str, kind: DataKind, sources: Vec<Source>) -> Self {
        Self {
            name,
            kind,
            sources,
            aliases: vec![],
            rename: None,
        }
    }
    pub fn add_source(mut self, source: Source) -> Self {
        self.sources.push(source);
        self
    }

    pub fn set_aliases(mut self, aliases: Vec<&'static str>) -> Self {
        self.aliases = aliases;
        self
    }
    pub fn add_alias(mut self, alias: &'static str) -> Self {
        self.aliases.push(alias);
        self
    }
    pub fn rename(mut self, rename: &'static str) -> Self {
        self.rename = Some(rename);
        self
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Source {
    pub from: SourceFrom,
    pub format: SourceFormat,
}
impl Source {
    pub fn new(from: SourceFrom, format: SourceFormat) -> Self {
        Self { from, format }
    }
}

impl Metadata {
    pub fn new(name: &'static str, kind: DataKind) -> Self {
        Self {
            name,
            kind,
            default_sources: vec![],
            fields: Vec::with_capacity(8),
            rename_all: None,
        }
    }

    pub fn set_default_sources(mut self, default_sources: Vec<Source>) -> Self {
        self.default_sources = default_sources;
        self
    }

    pub fn set_fields(mut self, fields: Vec<Field>) -> Self {
        self.fields = fields;
        self
    }

    pub fn add_default_source(mut self, source: Source) -> Self {
        self.default_sources.push(source);
        self
    }

    pub fn add_field(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }

    pub fn rename_all(mut self, rename_all: RenameRule) -> Self {
        self.rename_all = Some(rename_all);
        self
    }
}
