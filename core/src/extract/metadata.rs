#[derive(Debug, Clone)]
pub enum DataKind {
    Enum,
    Struct,
}

#[derive(Debug, Copy, Clone)]
pub enum SourceFrom {
    Param,
    Query,
    Header,
    Body,
}
#[derive(Debug, Copy, Clone)]
pub enum SourceFormat {
    MultiMap,
    Json,
}
#[derive(Debug, Clone)]
pub struct Metadata {
    pub name: &'static str,
    pub kind: DataKind,
    pub default_source: Option<Source>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
            default_source: None,
            fields: Vec::with_capacity(8),
        }
    }

    pub fn default_source(mut self, source: Source) -> Self {
        self.default_source = Some(source);
        self
    }

    pub fn add_field(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }
}
