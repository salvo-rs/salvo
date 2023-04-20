use std::any::TypeId;

use crate::Operation;

pub struct Endpoint {
    pub type_id: fn() -> TypeId,
    pub operation: fn() -> Operation,
}

impl Endpoint {
    pub const fn new(type_id: fn() -> TypeId, operation: fn() -> Operation) -> Self {
        Self { type_id, operation }
    }
}

// pub trait ToOperation {
//     fn operation(&self) -> Operation;
// }


// pub fn add<F>(endpoint: Endpoint) where F: Fn() -> Endpoint + 'static {
//     ENDPOINTS.write().insert(endpoint.type_id.clone(), endpoint));
// }

pub fn get_operation(type_id: &TypeId) -> Option<Operation> {
    for endpoint in inventory::iter::<Endpoint> {
        if (endpoint.type_id)() == *type_id {
            return Some((endpoint.operation)());
        }
    }
    None
}
inventory::collect!(Endpoint);
