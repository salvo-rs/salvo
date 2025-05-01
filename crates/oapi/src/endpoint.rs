use std::any::TypeId;

use salvo_core::http::StatusCode;
use salvo_core::{prelude::StatusError, writing};

use crate::{Components, Operation, Response, ToResponse, ToResponses, ToSchema};

/// Represents an endpoint.
///
/// View [module level documentation](index.html) for more details.
pub struct Endpoint {
    /// The operation information of the endpoint.
    pub operation: Operation,
    /// The OpenApi components section of the endpoint.
    pub components: Components,
}

impl Endpoint {
    /// Create new `Endpoint` with given operation and components.
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
            StatusCode::CONTINUE,
            StatusCode::SWITCHING_PROTOCOLS,
            StatusCode::PROCESSING,
            StatusCode::OK,
            StatusCode::CREATED,
            StatusCode::ACCEPTED,
            StatusCode::NON_AUTHORITATIVE_INFORMATION,
            StatusCode::NO_CONTENT,
            StatusCode::RESET_CONTENT,
            StatusCode::PARTIAL_CONTENT,
            StatusCode::MULTI_STATUS,
            StatusCode::ALREADY_REPORTED,
            StatusCode::IM_USED,
            StatusCode::MULTIPLE_CHOICES,
            StatusCode::MOVED_PERMANENTLY,
            StatusCode::FOUND,
            StatusCode::SEE_OTHER,
            StatusCode::NOT_MODIFIED,
            StatusCode::USE_PROXY,
            StatusCode::TEMPORARY_REDIRECT,
            StatusCode::PERMANENT_REDIRECT,
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
            Response::new("Ok").add_content("text/plain", String::to_schema(components)),
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


// ----> support anyhow::Result
#[cfg(feature = "anyhow")]
impl<T> EndpointOutRegister for anyhow::Result<T>
where
    T: EndpointOutRegister + Send,
{
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        // 注册成功情况的响应
        T::register(components, operation);

        // 注册错误情况的响应
        // anyhow::Error 可能代表多种错误情况，我们将其映射为 500 内部服务器错误
        operation.responses.insert(
            "500",
            Response::new("Internal Server Error")
                .add_content("text/plain", String::to_schema(components)),
        );

        // 可选：添加其他可能的状态码
        operation.responses.insert(
            "400",
            Response::new("Bad Request")
                .add_content("text/plain", String::to_schema(components)),
        );

        // 添加对 StatusError 的支持，因为 anyhow::Error 可能会包装 StatusError
        StatusError::register(components, operation);
    }
}

// 特殊处理 anyhow::Result<()> 的情况
#[cfg(feature = "anyhow")]
impl EndpointOutRegister for anyhow::Result<()> {
    #[inline]
    fn register(components: &mut Components, operation: &mut Operation) {
        // 成功情况
        operation.responses.insert("200", Response::new("Ok"));

        // 错误情况
        operation.responses.insert(
            "500",
            Response::new("Internal Server Error")
                .add_content("text/plain", String::to_schema(components)),
        );

        // 可选：添加其他可能的状态码
        operation.responses.insert(
            "400",
            Response::new("Bad Request")
                .add_content("text/plain", String::to_schema(components)),
        );

        // 添加对 StatusError 的支持
        StatusError::register(components, operation);
    }
}
