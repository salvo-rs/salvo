use std::any::TypeId;

use crate::{Components, Operation};

pub struct Endpoint {
    pub operation: Operation,
    pub components: Components,
}

pub trait EndpointModifier {
    fn modify(compontents: &mut Components, operation: &mut Operation);
    fn modify_with_arg(compontents: &mut Components, operation: &mut Operation, _arg: &str) {
        Self::modify(compontents, operation);
    }
}

pub struct EndpointRegistry {
    pub type_id: fn() -> TypeId,
    pub creator: fn() -> Endpoint,
}

impl EndpointRegistry {
    pub const fn save(type_id: fn() -> TypeId, creator: fn() -> Endpoint) -> Self {
        Self { type_id, creator }
    }
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
