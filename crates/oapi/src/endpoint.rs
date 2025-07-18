use std::any::TypeId;
use std::fmt::{self, Debug, Formatter};

use salvo_core::http::StatusCode;
use salvo_core::{prelude::StatusError, writing};

use crate::{Components, Operation, Response, ToResponse, ToResponses, ToSchema};

/// Represents an endpoint.
///
/// View [module level documentation](index.html) for more details.
#[derive(Clone, Debug)]
pub struct Endpoint {
    /// The operation information of the endpoint.
    pub operation: Operation,
    /// The OpenApi components section of the endpoint.
    pub components: Components,
}

impl Endpoint {
    /// Create new `Endpoint` with given operation and components.
    #[must_use]
    pub fn new(operation: Operation, components: Components) -> Self {
        Self {
            operation,
            components,
        }
    }
}

/// A trait for endpoint argument register.
pub trait EndpointArgRegister {
    /// Modify the OpenApi components section or current operation information with given argument. This function is called by macros internal.
    fn register(components: &mut Components, operation: &mut Operation, arg: &str);
}
/// A trait for endpoint return type register.
pub trait EndpointOutRegister {
    /// Modify the OpenApi components section or current operation information with given argument. This function is called by macros internal.
    fn register(components: &mut Components, operation: &mut Operation);
}

impl<C> EndpointOutRegister for writing::Json<C>
where
    C: ToSchema,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("200", Self::to_response(components));
    }
}
impl<T, E> EndpointOutRegister for Result<T, E>
where
    T: EndpointOutRegister + Send,
    E: EndpointOutRegister + Send,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        T::register(components, operation);
        E::register(components, operation);
    }
}
impl<E> EndpointOutRegister for Result<(), E>
where
    E: EndpointOutRegister + Send,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation.responses.insert("200", Response::new("Ok"));
        E::register(components, operation);
    }
}

impl EndpointOutRegister for StatusError {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .append(&mut Self::to_responses(components));
    }
}
impl EndpointOutRegister for StatusCode {
    fn register(components: &mut Components, operation: &mut Operation) {
        for code in [
            Self::CONTINUE,
            Self::SWITCHING_PROTOCOLS,
            Self::PROCESSING,
            Self::OK,
            Self::CREATED,
            Self::ACCEPTED,
            Self::NON_AUTHORITATIVE_INFORMATION,
            Self::NO_CONTENT,
            Self::RESET_CONTENT,
            Self::PARTIAL_CONTENT,
            Self::MULTI_STATUS,
            Self::ALREADY_REPORTED,
            Self::IM_USED,
            Self::MULTIPLE_CHOICES,
            Self::MOVED_PERMANENTLY,
            Self::FOUND,
            Self::SEE_OTHER,
            Self::NOT_MODIFIED,
            Self::USE_PROXY,
            Self::TEMPORARY_REDIRECT,
            Self::PERMANENT_REDIRECT,
        ] {
            operation.responses.insert(
                code.as_str(),
                Response::new(
                    code.canonical_reason()
                        .unwrap_or("No further explanation is available."),
                ),
            )
        }
        operation
            .responses
            .append(&mut StatusError::to_responses(components));
    }
}
impl EndpointOutRegister for salvo_core::Error {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .append(&mut Self::to_responses(components));
    }
}

impl EndpointOutRegister for &str {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation.responses.insert(
            "200",
            Response::new("Ok").add_content("text/plain", String::to_schema(components)),
        );
    }
}
impl EndpointOutRegister for String {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation.responses.insert(
            "200",
            Response::new("Ok").add_content("text/plain", Self::to_schema(components)),
        );
    }
}
impl EndpointOutRegister for &String {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        operation.responses.insert(
            "200",
            Response::new("Ok").add_content("text/plain", String::to_schema(components)),
        );
    }
}

/// A registry for all endpoints.
#[doc(hidden)]
#[non_exhaustive]
pub struct EndpointRegistry {
    /// The type id of the endpoint.
    pub type_id: fn() -> TypeId,
    /// The creator of the endpoint.
    pub creator: fn() -> Endpoint,
}

impl Debug for EndpointRegistry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("EndpointRegistry").finish()
    }
}

impl EndpointRegistry {
    /// Save the endpoint information to the registry.
    pub const fn save(type_id: fn() -> TypeId, creator: fn() -> Endpoint) -> Self {
        Self { type_id, creator }
    }
    /// Find the endpoint information from the registry.
    #[must_use]
    pub fn find(type_id: &TypeId) -> Option<fn() -> Endpoint> {
        for record in inventory::iter::<Self> {
            if (record.type_id)() == *type_id {
                return Some(record.creator);
            }
        }
        None
    }
}
inventory::collect!(EndpointRegistry);

#[cfg(feature = "anyhow")]
impl EndpointOutRegister for anyhow::Error {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        StatusError::register(components, operation);
    }
}

#[cfg(feature = "eyre")]
impl EndpointOutRegister for eyre::Report {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        StatusError::register(components, operation);
    }
}
