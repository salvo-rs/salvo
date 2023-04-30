use std::any::TypeId;

use crate::{Components, Operation};
use salvo_core::writer;

/// Represents an endpoint.
pub struct Endpoint {
    /// The operation information of the endpoint.
    pub operation: Operation,
    /// The OpenApi components section of the endpoint.
    pub components: Components,
}

/// A trait for endpoint argument register.
pub trait EndpointArgRegister {
    /// Modify the OpenApi compontents section or current operation information with given argument. This function is called by macros internal.
    fn register(compontents: &mut Components, operation: &mut Operation, arg: &str);
}
/// A trait for endpoint return type register.
pub trait EndpointOutRegister {
    /// Modify the OpenApi compontents section or current operation information with given argument. This function is called by macros internal.
    fn register(compontents: &mut Components, operation: &mut Operation);
}

impl<C> EndpointOutRegister for writer::Json<C>
where
    C: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation) {
        if let (Some(symbol), schema) = <C as ToSchema>::to_schema() {
            components.schemas.insert(symbol, schema);
        }
        operation.responses = C::to_responses();
    }
}


/// A registry for all endpoints.
pub struct EndpointRegistry {
    /// The type id of the endpoint.
    pub type_id: fn() -> TypeId,
    /// The creator of the endpoint.
    pub creator: fn() -> Endpoint,
}

impl EndpointRegistry {
    /// Save the endpoint information to the registry.
    pub const fn save(type_id: fn() -> TypeId, creator: fn() -> Endpoint) -> Self {
        Self { type_id, creator }
    }
    /// Find the endpoint information from the registry.
    pub fn find(type_id: &TypeId) -> Option<fn() -> Endpoint> {
        for record in inventory::iter::<EndpointRegistry> {
            if (record.type_id)() == *type_id {
                return Some(record.creator);
            }
        }
        None
    }
}
inventory::collect!(EndpointRegistry);
