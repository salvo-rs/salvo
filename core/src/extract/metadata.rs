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
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum SourceFormat {
    MultiMap,
    Json,
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

#[derive(Clone, Debug)]
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

    pub fn add_default_source(mut self, source: Source) -> Self {
        self.default_sources.push(source);
        self
    }

    pub fn add_field(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }
}
