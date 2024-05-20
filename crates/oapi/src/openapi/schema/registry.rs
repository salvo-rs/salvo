
// /// A registry for all schema names.
// #[doc(hidden)]
// #[non_exhaustive]
// pub struct NameRegistry {
//     /// The type id of the schema.
//     pub type_id: fn() -> TypeId,
//     /// The name of the schema.
//     pub name: &'static str,
// }

// impl NameRegistry {
//     /// Save the schema name to the registry.
//     pub const fn save(type_id: fn() -> TypeId, name: &'static str) -> Self {
//         Self { type_id, name }
//     }
//     /// Find the schema name from the registry.
//     pub fn find(type_id: &TypeId) -> Option<&'static str> {
//         for record in inventory::iter::<NameRegistry> {
//             if (record.type_id)() == *type_id {
//                 return Some(record.name);
//             }
//         }
//         None
//     }
// }
// inventory::collect!(NameRegistry);

static GLOBAL_NAMER: RwLock<Option<Box<dyn Namer>>> = RwLock::new();

pub fn set_global_namer(namer: Box<dyn Namer>) {
    *GLOBAL_NAMER.write().unwrap() = Some(namer);
}

pub fn get_name<T>() -> Option<&'static str> {

}
pub fn set_name<T>(name: Option<&'static str>) {
}

pub trait Namer {
    fn get(type_id: &TypeId) -> Option<&'static str>;
}

pub struct DefaultNamer;