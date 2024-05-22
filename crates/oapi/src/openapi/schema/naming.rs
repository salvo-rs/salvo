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

static GLOBAL_NAMER: Lazy<RwLock<Box<dyn Namer>>> = Lazy::new(|| RwLock::new(Box::new(WordyNamer::new())));
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
    namer().assign_name(type_id, type_name, rule)
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
    let re = Regex::new(r"^[^<]+").unwrap();
    let result = re.replace_all(type_name, "");
    result.to_string()
}   
/// Namer is used to assign names to types.
pub trait Namer: Sync + Send + 'static {
    /// Assign name to type.
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String;
}

/// A namer that generates wordy names.
pub struct WordyNamer;
impl WordyNamer {
    /// Create a new WordyNamer.
    pub fn new() -> Self {
        Self
    }
}
impl Namer for WordyNamer {
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String {
        let name = match rule {
            NameRule::Auto => {
                let base = type_name.replace("::", ".");
                let mut name = base.to_string();
                let mut count = 1;
                while type_info_by_name(&name).map(|t| t.0) == Some(type_id) {
                    name = format!("{}{}", base, count);
                    count += 1;
                }
                name
            }
            NameRule::Force(name) => {
                let name = format!{"{}{}", name, type_generic_part(type_name)};
                if let Some((exist_id, exist_name)) = type_info_by_name(&name) {
                    if exist_id != type_id {
                        panic!("Duplicate name for types: {}, {}", exist_name, type_name);
                    }
                }
                name.to_string()
            }
        };
        set_name_type_info(name.clone(), type_id, type_name);
        name
    }
}

/// A namer that generates short names.
pub struct ShortNamer;
impl ShortNamer {
    /// Create a new ShortNamer.
    pub fn new() -> Self {
        Self
    }
}
impl Namer for ShortNamer {
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String {
        let name: String = match rule {
            NameRule::Auto => {
                let re = Regex::new(r"([^:<>]+::)+").unwrap();
                let base = re.replace_all(type_name, "");
                let mut name = base.to_string();
                let mut count = 1;
                while type_info_by_name(&name).map(|t| t.0) == Some(type_id) {
                    name = format!("{}{}", base, count);
                    count += 1;
                }
                name
            }
            NameRule::Force(name) => {
                let name = format!{"{}{}", name, type_generic_part(type_name)};
                if let Some((exist_id, exist_name)) = type_info_by_name(&name) {
                    if exist_id != type_id {
                        panic!("Duplicate name for types: {}, {}", exist_name, type_name);
                    }
                }
                name.to_string()
            }
        };
        set_name_type_info(name.clone(), type_id, type_name);
        name
    }
}
