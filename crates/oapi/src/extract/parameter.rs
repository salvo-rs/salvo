//! Request parameter extractors for the API operation.

pub use salvo_core::extract::{CookieParam, HeaderParam, PathParam, QueryParam};

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Operation, Parameter, ParameterIn, ToSchema};

impl<T, const REQUIRED: bool> EndpointArgRegister for CookieParam<T, REQUIRED>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg)
            .location(ParameterIn::Cookie)
            .description(format!("Get parameter `{arg}` from request cookie."))
            .schema(T::to_schema(components))
            .required(REQUIRED);
        operation.parameters.insert(parameter);
    }
}

impl<T, const REQUIRED: bool> EndpointArgRegister for HeaderParam<T, REQUIRED>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg)
            .location(ParameterIn::Header)
            .description(format!("Get parameter `{arg}` from request headers."))
            .schema(T::to_schema(components))
            .required(REQUIRED);
        operation.parameters.insert(parameter);
    }
}

impl<T> EndpointArgRegister for PathParam<T>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg)
            .location(ParameterIn::Path)
            .description(format!("Get parameter `{arg}` from request url path."))
            .schema(T::to_schema(components))
            .required(true);
        operation.parameters.insert(parameter);
    }
}

impl<T, const REQUIRED: bool> EndpointArgRegister for QueryParam<T, REQUIRED>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg)
            .location(ParameterIn::Query)
            .description(format!("Get parameter `{arg}` from request url query."))
            .schema(T::to_schema(components))
            .required(REQUIRED);
        operation.parameters.insert(parameter);
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_cookie_param_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        CookieParam::<String, false>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "parameters": [{
                    "name": "arg",
                    "in": "cookie",
                    "description": "Get parameter `arg` from request cookie.",
                    "required": false,
                    "schema": { "type": "string" }
                }],
                "responses": {}
            })
        );
    }

    #[test]
    fn test_header_param_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        HeaderParam::<String, false>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "parameters": [{
                    "name": "arg",
                    "in": "header",
                    "description": "Get parameter `arg` from request headers.",
                    "required": false,
                    "schema": { "type": "string" }
                }],
                "responses": {}
            })
        );
    }

    #[test]
    fn test_path_param_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        PathParam::<String>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "parameters": [{
                    "name": "arg",
                    "in": "path",
                    "description": "Get parameter `arg` from request url path.",
                    "required": true,
                    "schema": { "type": "string" }
                }],
                "responses": {}
            })
        );
    }

    #[test]
    fn test_query_param_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        QueryParam::<String, false>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "parameters": [{
                    "name": "arg",
                    "in": "query",
                    "description": "Get parameter `arg` from request url query.",
                    "required": false,
                    "schema": { "type": "string" }
                }],
                "responses": {}
            })
        );
    }
}
