use std::any::TypeId;
use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use regex::Regex;

/// A registry for all schema names.
#[doc(hidden)]
#[non_exhaustive]
pub struct NameRuleRegistry {
    /// The type id of the schema.
    pub type_id: fn() -> TypeId,
    /// The name of the schema.
    pub rule: NameRule,
}

#[derive(Debug, Clone, Copy)]
pub enum NameRule {
    Trans,
    Const(&'static str),
}

impl NameRuleRegistry {
    /// Save the schema name to the registry.
    pub const fn save(type_id: fn() -> TypeId, rule: NameRule) -> Self {
        Self { type_id, rule }
    }
    /// Find the schema name from the registry.
    pub fn find(type_id: &TypeId) -> Option<NameRule> {
        for record in inventory::iter::<NameRuleRegistry> {
            if (record.type_id)() == *type_id {
                return Some(record.rule);
            }
        }
        None
    }
}
inventory::collect!(NameRuleRegistry);

static GLOBAL_NAMER: Lazy<RwLock<Box<dyn Namer>>> = Lazy::new(|| RwLock::new(Box::new(WordyNamer::new())));
static GLOBAL_NAMES: Lazy<RwLock<BTreeMap<String, (TypeId, &'static str)>>> = Lazy::new(Default::default);

pub fn set_namer(namer: impl Namer) {
    *GLOBAL_NAMER.write() = Box::new(namer);
}

pub fn name_type(name: &str) -> Option<(TypeId, &'static str)> {
    GLOBAL_NAMES.read().get(name).cloned()
}
pub fn set_name_type(name: String, type_id: TypeId, type_name: &'static str) -> Option<(TypeId, &'static str)> {
    GLOBAL_NAMES.write().insert(name.clone(), (type_id, type_name))
}

pub trait Namer: Sync + Send + 'static {
    fn name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String;
}

pub struct WordyNamer;
impl WordyNamer {
    pub fn new() -> Self {
        Self
    }
}
impl Namer for WordyNamer {
    fn name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String {
        let name = match rule {
            NameRule::Trans => {
                let base = type_name.replace("::", ".").replace('<', "L").replace('>', "7");
                let mut name = base.clone();
                let mut count = 1;
                while name_type(&name).map(|t| t.0) == Some(type_id) {
                    name = format!("{}{}", base, count);
                    count += 1;
                }
                name
            }
            NameRule::Const(name) => {
                if let Some((exist_id, exist_name)) = name_type(name) {
                    if exist_id != type_id {
                        panic!("Duplicate name for types: {}, {}", exist_name, type_name);
                    }
                }
                name.to_string()
            }
        };
        set_name_type(name.clone(), type_id, type_name);
        name
    }
}

pub struct ShortNamer;
impl ShortNamer {
    pub fn new() -> Self {
        Self
    }
}
impl Namer for ShortNamer {
    fn name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String {
        let name = match rule {
            NameRule::Trans => {
                let re = Regex::new(r"([^:<>]+::)+").unwrap();
                let mut base = re.replace_all(type_name, "").replace('<', "L").replace('>', "7");
                let mut name = base.clone();
                let mut count = 1;
                while name_type(&name).map(|t| t.0) == Some(type_id) {
                    name = format!("{}{}", base, count);
                    count += 1;
                }
                name
            }
            NameRule::Const(name) => {
                if let Some((exist_id, exist_name)) = name_type(name) {
                    if exist_id !=type_id {
                        panic!("Duplicate name for types: {}, {}", exist_name, type_name);
                    }
                }
                name.to_string()
            }
        };
        set_name_type(name.clone(), type_id, type_name);
        name
    }
}
