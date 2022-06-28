#[derive(Debug, Clone)]
pub enum DataKind {
    Empty,
    Unit,
    Struct,
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
    pub type_name: &'static str,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub from: &'static str,
    pub format: &'static str,
}
impl Source {
    pub fn new(from: &'static str, format: &'static str) -> Self {
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

    pub fn add_field(mut self, name: &'static str, type_name: &'static str, sources: Vec<Source>) -> Self {
        self.fields.push(Field {
            name,
            type_name,
            sources,
        });
        self
    }
}
