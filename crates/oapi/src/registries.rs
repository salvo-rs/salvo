use std::any::TypeId;

use crate::{Operation, Schema};

pub struct OperationRegistry {
    pub type_id: fn() -> TypeId,
    pub operation: fn() -> Operation,
}

impl OperationRegistry {
    pub const fn save(type_id: fn() -> TypeId, operation: fn() -> Operation) -> Self {
        Self { type_id, operation }
    }
    pub fn find(type_id: &TypeId) -> Option<Operation> {
        for record in inventory::iter::<OperationRegistry> {
            if (record.type_id)() == *type_id {
                return Some((record.operation)());
            }
        }
        None
    }
}
inventory::collect!(OperationRegistry);

pub struct SchemaRegistry {
    pub type_id: fn() -> TypeId,
    pub schema: fn() -> Schema,
}

impl SchemaRegistry {
    pub const fn save(type_id: fn() -> TypeId, schema: fn() -> Schema) -> Self {
        Self { type_id, schema }
    }
    pub fn find(type_id: &TypeId) -> Option<Schema> {
        for record in inventory::iter::<SchemaRegistry> {
            if (record.type_id)() == *type_id {
                return Some((record.schema)());
            }
        }
        None
    }
}
inventory::collect!(SchemaRegistry);
