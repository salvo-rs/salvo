use std::any::TypeId;

use crate::{Components, Operation};

/// Represents an endpoint.
pub struct Endpoint {
    /// The operation information of the endpoint.
    pub operation: Operation,
    /// The OpenApi components section of the endpoint.
    pub components: Components,
}

/// A trait for endpoint modifier.
pub trait EndpointModifier {
    /// Modify the OpenApi compontents section or current operation information.
    fn modify(compontents: &mut Components, operation: &mut Operation);
    /// Modify the OpenApi compontents section or current operation information with given argument. This function is called by macros internal.
    #[doc(hidden)]
    fn modify_with_arg(compontents: &mut Components, operation: &mut Operation, _arg: &str) {
        Self::modify(compontents, operation);
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
