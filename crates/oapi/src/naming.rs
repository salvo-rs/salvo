use std::any::TypeId;
use std::collections::BTreeMap;
use std::sync::LazyLock;

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

static GLOBAL_NAMER: LazyLock<RwLock<Box<dyn Namer>>> =
    LazyLock::new(|| RwLock::new(Box::new(FlexNamer::new())));
static NAME_TYPES: LazyLock<RwLock<BTreeMap<String, (TypeId, &'static str)>>> =
    LazyLock::new(Default::default);

/// Set global namer.
///
/// Set global namer, all the types will be named by this namer. You should call this method before
/// at before you generate OpenAPI schema.
///
/// # Example
///
/// ```rust
/// # use salvo_oapi::extract::*;
/// # use salvo_core::prelude::*;
/// # #[tokio::main]
/// # async fn main() {
/// salvo_oapi::naming::set_namer(
///     salvo_oapi::naming::FlexNamer::new()
///         .short_mode(true)
///         .generic_delimiter('_', '_'),
/// );
/// # }
/// ```
pub fn set_namer(namer: impl Namer) {
    *GLOBAL_NAMER.write() = Box::new(namer);
    NAME_TYPES.write().clear();
}

#[doc(hidden)]
pub fn namer() -> RwLockReadGuard<'static, Box<dyn Namer>> {
    GLOBAL_NAMER.read()
}

/// Get type info by name.
pub fn type_info_by_name(name: &str) -> Option<(TypeId, &'static str)> {
    NAME_TYPES.read().get(name).cloned()
}

/// Get registered name by rust type name (from `std::any::type_name`).
///
/// This searches through NAME_TYPES to find if a type with the given rust type name
/// has been registered with a custom name.
pub fn name_by_type_name(type_name: &str) -> Option<String> {
    NAME_TYPES
        .read()
        .iter()
        .find(|(_, (_, registered_type_name))| *registered_type_name == type_name)
        .map(|(name, _)| name.clone())
}

/// Resolve generic type parameters to their registered names.
///
/// This function recursively processes a type name string and replaces any
/// generic type parameters with their registered names from NAME_TYPES.
///
/// For example, if `CityDTO` is registered as `City`, then:
/// - `Response<CityDTO>` becomes `Response<City>`
/// - `Vec<HashMap<String, CityDTO>>` becomes `Vec<HashMap<String, City>>`
pub fn resolve_generic_names(type_name: &str) -> String {
    // First check if the entire type (without generics) has a registered name
    if let Some(registered_name) = name_by_type_name(type_name) {
        return registered_name;
    }

    // Find the position of the first '<' to separate base type from generic params
    let Some(generic_start) = type_name.find('<') else {
        // No generics, return as-is
        return type_name.to_string();
    };

    // Extract base type and generic part
    let base_type = &type_name[..generic_start];
    let generic_part = &type_name[generic_start..];

    // Parse and resolve each generic parameter
    let resolved_generic = resolve_generic_part(generic_part);

    format!("{base_type}{resolved_generic}")
}

/// Parse generic part like `<A, B<C, D>, E>` and resolve each type parameter.
fn resolve_generic_part(generic_part: &str) -> String {
    if !generic_part.starts_with('<') || !generic_part.ends_with('>') {
        return generic_part.to_string();
    }

    // Remove outer < and >
    let inner = &generic_part[1..generic_part.len() - 1];

    // Split by top-level commas (not nested in <>)
    let params = split_generic_params(inner);

    let resolved_params: Vec<String> = params
        .into_iter()
        .map(|param| {
            let param = param.trim();
            // Check if this exact type has a registered name
            if let Some(registered_name) = name_by_type_name(param) {
                registered_name
            } else if param.contains('<') {
                // Recursively resolve nested generics
                resolve_generic_names(param)
            } else {
                param.to_string()
            }
        })
        .collect();

    format!("<{}>", resolved_params.join(", "))
}

/// Split generic parameters at top-level commas, respecting nested angle brackets.
fn split_generic_params(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    // Don't forget the last segment
    if start < s.len() {
        result.push(&s[start..]);
    }

    result
}

/// Set type info by name.
pub fn set_name_type_info(
    name: String,
    type_id: TypeId,
    type_name: &'static str,
) -> Option<(TypeId, &'static str)> {
    NAME_TYPES.write().insert(name, (type_id, type_name))
}

/// Assign name to type and returns the name.
///
/// If the type is already named, return the existing name.
pub fn assign_name<T: 'static>(rule: NameRule) -> String {
    let type_id = TypeId::of::<T>();
    let type_name = std::any::type_name::<T>();
    for (name, (exist_id, _)) in NAME_TYPES.read().iter() {
        if *exist_id == type_id {
            return name.clone();
        }
    }
    namer().assign_name(type_id, type_name, rule)
}

/// Get the name of the type. Panic if the name is not exist.
pub fn get_name<T: 'static>() -> String {
    let type_id = TypeId::of::<T>();
    for (name, (exist_id, _)) in NAME_TYPES.read().iter() {
        if *exist_id == type_id {
            return name.clone();
        }
    }
    panic!(
        "Type not found in the name registry: {:?}",
        std::any::type_name::<T>()
    );
}

fn type_generic_part(type_name: &str) -> String {
    if let Some(pos) = type_name.find('<') {
        type_name[pos..].to_string()
    } else {
        String::new()
    }
}

/// Resolve generic part and format it according to namer settings.
fn resolve_and_format_generic_part(type_name: &str, short_mode: bool) -> String {
    let generic_part = type_generic_part(type_name);
    if generic_part.is_empty() {
        return generic_part;
    }

    // Resolve registered names in generic parameters
    let resolved = resolve_generic_part(&generic_part);

    // Apply formatting (:: -> . for non-short mode, or strip module paths for short mode)
    if short_mode {
        let re = Regex::new(r"([^<>, ]*::)+").expect("Invalid regex");
        re.replace_all(&resolved, "").to_string()
    } else {
        resolved.replace("::", ".")
    }
}
/// Namer is used to assign names to types.
pub trait Namer: Sync + Send + 'static {
    /// Assign name to type.
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String;
}

/// A namer that generates wordy names.
#[derive(Default, Clone, Debug)]
pub struct FlexNamer {
    short_mode: bool,
    generic_delimiter: Option<(String, String)>,
}
impl FlexNamer {
    /// Create a new FlexNamer.
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the short mode.
    #[must_use]
    pub fn short_mode(mut self, short_mode: bool) -> Self {
        self.short_mode = short_mode;
        self
    }

    /// Set the delimiter for generic types.
    #[must_use]
    pub fn generic_delimiter(mut self, open: impl Into<String>, close: impl Into<String>) -> Self {
        self.generic_delimiter = Some((open.into(), close.into()));
        self
    }
}
impl Namer for FlexNamer {
    fn assign_name(&self, type_id: TypeId, type_name: &'static str, rule: NameRule) -> String {
        let name = match rule {
            NameRule::Auto => {
                // First resolve any registered names in generic parameters
                let resolved_type_name = resolve_generic_names(type_name);

                let mut base = if self.short_mode {
                    let re = Regex::new(r"([^<>, ]*::)+").expect("Invalid regex");
                    re.replace_all(&resolved_type_name, "").to_string()
                } else {
                    resolved_type_name.replace("::", ".")
                };
                if let Some((open, close)) = &self.generic_delimiter {
                    base = base.replace('<', open).replace('>', close);
                }
                let mut name = base.clone();
                let mut count = 1;
                while let Some(exist_id) = type_info_by_name(&name).map(|t| t.0) {
                    if exist_id != type_id {
                        count += 1;
                        name = format!("{base}{count}");
                    } else {
                        break;
                    }
                }
                name
            }
            NameRule::Force(force_name) => {
                // Resolve registered names in generic parameters
                let resolved_generic = resolve_and_format_generic_part(type_name, self.short_mode);

                let mut base = if self.short_mode {
                    // In short mode with Force, use the forced name + resolved generics
                    format!("{}{}", force_name, resolved_generic)
                } else {
                    format!("{}{}", force_name, resolved_generic)
                };
                if let Some((open, close)) = &self.generic_delimiter {
                    base = base.replace('<', open).replace('>', close);
                }
                let mut name = base.clone();
                let mut count = 1;
                while let Some((exist_id, exist_name)) = type_info_by_name(&name) {
                    if exist_id != type_id {
                        count += 1;
                        tracing::error!("Duplicate name for types: {}, {}", exist_name, type_name);
                        name = format!("{base}{count}");
                    } else {
                        break;
                    }
                }
                name
            }
        };
        set_name_type_info(name.clone(), type_id, type_name);
        name
    }
}

mod tests {
    #[test]
    fn test_name() {
        use super::*;

        struct MyString;
        mod nest {
            pub(crate) struct MyString;
        }

        let name = assign_name::<String>(NameRule::Auto);
        assert_eq!(name, "alloc.string.String");
        let name = assign_name::<Vec<String>>(NameRule::Auto);
        assert_eq!(name, "alloc.vec.Vec<alloc.string.String>");

        let name = assign_name::<MyString>(NameRule::Auto);
        assert_eq!(name, "salvo_oapi.naming.tests.test_name.MyString");
        let name = assign_name::<nest::MyString>(NameRule::Auto);
        assert_eq!(name, "salvo_oapi.naming.tests.test_name.nest.MyString");
    }

    #[test]
    fn test_resolve_generic_names() {
        use super::*;

        // Clear registry for this test
        NAME_TYPES.write().clear();

        // Simulate registering CityDTO as "City"
        let city_type_name = "test_module::CityDTO";
        set_name_type_info(
            "City".to_string(),
            TypeId::of::<()>(), // dummy TypeId
            city_type_name,
        );

        // Test resolve_generic_names
        let resolved = resolve_generic_names("Response<test_module::CityDTO>");
        assert_eq!(resolved, "Response<City>");

        // Test nested generics
        let resolved = resolve_generic_names("Vec<HashMap<String, test_module::CityDTO>>");
        assert_eq!(resolved, "Vec<HashMap<String, City>>");

        // Test multiple generic parameters
        let resolved = resolve_generic_names("Tuple<test_module::CityDTO, test_module::CityDTO>");
        assert_eq!(resolved, "Tuple<City, City>");
    }

    #[test]
    fn test_split_generic_params() {
        use super::*;

        let params = split_generic_params("A, B, C");
        assert_eq!(params, vec!["A", " B", " C"]);

        let params = split_generic_params("A<X, Y>, B, C<Z>");
        assert_eq!(params, vec!["A<X, Y>", " B", " C<Z>"]);

        let params = split_generic_params("A<X<Y, Z>>, B");
        assert_eq!(params, vec!["A<X<Y, Z>>", " B"]);
    }

    #[test]
    fn test_assign_name_with_generic_resolution() {
        use super::*;

        // Reset namer to default state - this also clears NAME_TYPES
        set_namer(FlexNamer::new());

        // Define unique test types for this test to avoid conflicts with other tests
        mod test_generic_resolution {
            pub(super) struct CityDTO;
            pub(super) struct Response<T>(std::marker::PhantomData<T>);
            pub(super) struct Wrapper<T>(std::marker::PhantomData<T>);
        }
        use test_generic_resolution::*;

        // First, register CityDTO with a custom name "City"
        let city_name = assign_name::<CityDTO>(NameRule::Force("City"));
        assert_eq!(city_name, "City");

        // Now register Response<CityDTO> with Force("Response")
        // It should resolve CityDTO to "City" in the generic parameter
        let response_name = assign_name::<Response<CityDTO>>(NameRule::Force("Response"));
        assert_eq!(response_name, "Response<City>");

        // Test with Auto mode - should also resolve generic parameters
        let wrapper_name = assign_name::<Wrapper<CityDTO>>(NameRule::Auto);
        // The base type will have full path, but CityDTO should be resolved to City
        assert!(
            wrapper_name.contains("<City>"),
            "Expected wrapper name to contain '<City>', got: {}",
            wrapper_name
        );
    }
}
