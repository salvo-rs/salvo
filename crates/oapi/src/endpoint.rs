//! endpoint module

use std::any::TypeId;

use salvo_core::{
    prelude::{StatusCode, StatusError},
    writer,
};

use crate::{Components, Operation, Response, ToResponse, ToResponses, ToSchema};

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
    fn register(compontents: &mut Components, operation: &mut Operation, status_codes: &[StatusCode]);
}

impl<C> EndpointOutRegister for writer::Json<C>
where
    C: ToSchema,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation, status_codes: &[StatusCode]) {
        if status_codes.is_empty() || status_codes.contains(&StatusCode::OK) {
            operation.responses.insert("200", Self::to_response(components));
        }
    }
}
impl<T, E> EndpointOutRegister for Result<T, E>
where
    T: EndpointOutRegister + Send,
    E: EndpointOutRegister + Send,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation, status_codes: &[StatusCode]) {
        T::register(components, operation, status_codes);
        E::register(components, operation, status_codes);
    }
}
impl<E> EndpointOutRegister for Result<(), E>
where
    E: EndpointOutRegister + Send,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation, status_codes: &[StatusCode]) {
        if status_codes.is_empty() || status_codes.contains(&StatusCode::OK) {
            operation.responses.insert("200", Response::new("Ok"));
        }
        E::register(components, operation, status_codes);
    }
}

impl EndpointOutRegister for StatusError {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation, _status_codes: &[StatusCode]) {
        operation.responses.append(&mut Self::to_responses(components));
    }
}

/// A components for all endpoints.
pub struct EndpointRegistry {
    /// The type id of the endpoint.
    pub type_id: fn() -> TypeId,
    /// The creator of the endpoint.
    pub creator: fn() -> Endpoint,
}

impl EndpointRegistry {
    /// Save the endpoint information to the components.
    pub const fn save(type_id: fn() -> TypeId, creator: fn() -> Endpoint) -> Self {
        Self { type_id, creator }
    }
    /// Find the endpoint information from the components.
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
