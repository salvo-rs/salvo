use std::vec;
use std::str::FromStr;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum DataKind {
    Enum,
    Struct,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFrom {
    Param,
    Query,
    Header,
    Body,
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


#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFormat {
    MultiMap,
    Json,
}

impl FromStr for SourceFormat {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "multiMap" => Ok(Self::MultiMap),
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
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: &'static str,
    pub kind: DataKind,
    pub sources: Vec<Source>,
}
impl Field {
    pub fn new(name: &'static str, kind: DataKind) -> Self {
        Self::with_source(name, kind, vec![])
    }
    pub fn with_source(name: &'static str, kind: DataKind, sources: Vec<Source>) -> Self {
        Self {
            name,
            kind,
            sources: vec![],
        }
    }
    pub fn add_source(mut self, source: Source) -> Self {
        self.sources.push(source);
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
}
