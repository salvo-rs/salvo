use std::any::TypeId;
use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use parking_lot::{RwLock, RwLockReadGuard};
use regex::Regex;

/// NameRule is used to specify the rule of naming.
#[derive(Default, Debug, Clone, Copy)]
pub enum NameRule {
    /// Auto generate name by namer.
    #[default]
    Auto,
    /// Force to use the given name.
    Force(&'static str),
}

static GLOBAL_NAMER: Lazy<RwLock<Box<dyn Namer>>> = Lazy::new(|| RwLock::new(Box::new(FlexNamer::new())));
static GLOBAL_NAMES: Lazy<RwLock<BTreeMap<String, (TypeId, &'static str)>>> = Lazy::new(Default::default);

/// Set global namer.
pub fn set_namer(namer: impl Namer) {
    *GLOBAL_NAMER.write() = Box::new(namer);
}

#[doc(hidden)]
pub fn namer() -> RwLockReadGuard<'static, Box<dyn Namer>> {
    GLOBAL_NAMER.read()
}

fn type_info_by_name(name: &str) -> Option<(TypeId, &'static str)> {
    GLOBAL_NAMES.read().get(name).cloned()
}
fn set_name_type_info(name: String, type_id: TypeId, type_name: &'static str) -> Option<(TypeId, &'static str)> {
    GLOBAL_NAMES.write().insert(name.clone(), (type_id, type_name))
}

/// Assign name to type and returns the name. If the type is already named, return the existing name.
pub fn assign_name<T: 'static>(rule: NameRule) -> String {
    let type_id = TypeId::of::<T>();
    let type_name = std::any::type_name::<T>();
    for (name, (exist_id, _)) in GLOBAL_NAMES.read().iter() {
        if *exist_id == type_id {
            return name.clone();
        }
    }
    let name = namer().assign_name(type_id, type_name, rule);
    set_name_type_info(name.clone(), type_id, type_name);
    name
}

/// Get the name of the type. Panic if the name is not exist.
pub fn get_name<T: 'static>() -> String {
    let type_id = TypeId::of::<T>();
    for (name, (exist_id, _)) in GLOBAL_NAMES.read().iter() {
        if *exist_id == type_id {
            return name.clone();
        }
    }
    panic!("Type not found in the name registry: {:?}", std::any::type_name::<T>());
}

fn type_generic_part(type_name: &str) -> String {
    let re = Regex::new(r"^[^<]+").expect("Invalid regex");
    let result = re.replace_all(type_name, "");
    result.to_string()
}
/// Namer is used to assign names to types.
pub trait Namer: Sync + Send + 'static {
    /// Assign name to type.
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String;
}

/// A namer that generates wordy names.
#[derive(Clone, Debug)]
pub struct FlexNamer {
    short_mode: bool,
    generic_delimiter: Option<(String, String)>,
}
impl Default for FlexNamer {
    fn default() -> Self {
        Self {
            short_mode: false,
            generic_delimiter: None,
        }
    }
}
impl FlexNamer {
    /// Create a new FlexNamer.
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the short mode.
    pub fn short_mode(mut self, short_mode: bool) -> Self {
        self.short_mode = short_mode;
        self
    }
    
    /// Set the delimiter for generic types.
    pub fn generic_delimiter(mut self, open: impl Into<String>, close: impl Into<String>) -> Self {
        self.generic_delimiter = Some((open.into(), close.into()));
        self
    }
}
impl Namer for FlexNamer {
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String {
        match rule {
            NameRule::Auto => {
                let mut base = if self.short_mode {
                    let re = Regex::new(r"([^<>]*::)+").expect("Invalid regex");
                    re.replace_all(type_name, "").to_string()
                } else {
                    type_name.replace("::", ".")
                };
                if let Some((open, close)) = &self.generic_delimiter {
                    base = base.replace('<', open).replace('>', close);
                }
                let mut name = base.to_string();
                let mut count = 1;
                while type_info_by_name(&name).map(|t| t.0) == Some(type_id) {
                    name = format!("{}{}", base, count);
                    count += 1;
                }
                name
            }
            NameRule::Force(name) => {
                let mut name =  if self.short_mode {
                    let re = Regex::new(r"([^<>]*::)+").expect("Invalid regex");
                    re.replace_all(type_name, "").to_string()
                } else {
                    format! {"{}{}", name, type_generic_part(type_name).replace("::", ".")}
                };
                if let Some((open, close)) = &self.generic_delimiter {
                    name = name.replace('<', open).replace('>', close).to_string();
                }
                if let Some((exist_id, exist_name)) = type_info_by_name(&name) {
                    if exist_id != type_id {
                        panic!("Duplicate name for types: {}, {}", exist_name, type_name);
                    }
                }
                name.to_string()
            }
        }
    }
}
