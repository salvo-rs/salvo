//! Request body extractors for the API operation.

pub use salvo_core::extract::{FormBody, FormFile, FormFiles, JsonBody};
use serde::Deserialize;

use crate::endpoint::EndpointArgRegister;
use crate::{
    Array, BasicType, Components, Content, KnownFormat, Object, Operation, RequestBody, Schema,
    SchemaFormat, ToRequestBody, ToSchema,
};

impl EndpointArgRegister for FormFile {
    fn register(_components: &mut Components, operation: &mut Operation, arg: &str) {
        let schema = Schema::from(
            Object::new().property(
                arg,
                Object::with_type(BasicType::String)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Binary)),
            ),
        );

        if let Some(request_body) = &mut operation.request_body {
            request_body
                .contents
                .insert("multipart/form-data".into(), Content::new(schema));
        } else {
            let request_body = RequestBody::new()
                .description("Upload a file.")
                .add_content("multipart/form-data", Content::new(schema));
            operation.request_body = Some(request_body);
        }
    }
}

impl EndpointArgRegister for FormFiles {
    fn register(_components: &mut Components, operation: &mut Operation, arg: &str) {
        let schema = Schema::from(
            Object::new().property(
                arg,
                Array::new().items(Schema::from(
                    Object::with_type(BasicType::String)
                        .format(SchemaFormat::KnownFormat(KnownFormat::Binary)),
                )),
            ),
        );
        if let Some(request_body) = &mut operation.request_body {
            request_body
                .contents
                .insert("multipart/form-data".into(), Content::new(schema));
        } else {
            let request_body = RequestBody::new()
                .description("Upload files.")
                .add_content("multipart/form-data", Content::new(schema));
            operation.request_body = Some(request_body);
        }
    }
}

impl<'de, T> ToRequestBody for FormBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn to_request_body(components: &mut Components) -> RequestBody {
        let schema = T::to_schema(components);
        RequestBody::new()
            .description("Extract form format data from request.")
            .add_content(
                "application/x-www-form-urlencoded",
                Content::new(schema.clone()),
            )
            // Keep the form schema separate from the file schema registered under
            // `multipart/form-data` until those schemas can be merged correctly.
            .add_content("multipart/*", Content::new(schema))
    }
}

impl<'de, T> EndpointArgRegister for FormBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        operation.request_body = Some(Self::to_request_body(components));
    }
}

impl<'de, T> ToRequestBody for JsonBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn to_request_body(components: &mut Components) -> RequestBody {
        RequestBody::new()
            .description("Extract json format data from request.")
            .add_content("application/json", Content::new(T::to_schema(components)))
    }
}

impl<'de, T> EndpointArgRegister for JsonBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        let request_body = Self::to_request_body(components);
        let _ = <T as ToSchema>::to_schema(components);
        operation.request_body = Some(request_body);
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_form_body_to_request_body() {
        let mut components = Components::default();
        let request_body = FormBody::<String>::to_request_body(&mut components);
        assert_json_eq!(
            request_body,
            json!({
                "description": "Extract form format data from request.",
                "content": {
                    "application/x-www-form-urlencoded": {
                        "schema": { "type": "string" }
                    },
                    "multipart/*": {
                        "schema": { "type": "string" }
                    }
                }
            })
        );
    }

    #[test]
    fn test_form_body_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        FormBody::<String>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "requestBody": {
                    "content": {
                        "application/x-www-form-urlencoded": {
                            "schema": { "type": "string" }
                        },
                        "multipart/*": {
                            "schema": { "type": "string" }
                        }
                    },
                    "description": "Extract form format data from request."
                },
                "responses": {}
            })
        );
    }

    #[test]
    fn test_json_body_to_request_body() {
        let mut components = Components::default();
        let request_body = JsonBody::<String>::to_request_body(&mut components);
        assert_json_eq!(
            request_body,
            json!({
                "description": "Extract json format data from request.",
                "content": {
                    "application/json": {
                        "schema": { "type": "string" }
                    }
                }
            })
        );
    }

    #[test]
    fn test_json_body_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        JsonBody::<String>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "requestBody": {
                    "content": {
                        "application/json": {
                            "schema": { "type": "string" }
                        }
                    },
                    "description": "Extract json format data from request."
                },
                "responses": {}
            })
        );
    }
}
