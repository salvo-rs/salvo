#[derive(Debug, Clone)]
pub enum DataKind {
    Empty,
    Unit,
    Struct,
}
#[derive(Debug, Clone)]
pub struct Metadata {
    pub name: String,
    pub kind: DataKind,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub format: String,
}
impl Source {
    pub fn new(name: &str, format: &str) -> Self {
        Self {
            name: name.to_string(),
            format: format.to_string(),
        }
    }
}

impl Metadata {
    pub fn new(name: impl Into<String>, kind: DataKind) -> Self {
        Self {
            name: name.into(),
            kind,
            fields: Vec::with_capacity(8),
        }
    }

    pub fn add_field(mut self, name: impl Into<String>, type_name: impl Into<String>, sources: Vec<Source>) -> Self {
        self.fields.push(Field {
            name: name.into(),
            type_name: type_name.into(),
            sources,
        });
        self
    }
}
