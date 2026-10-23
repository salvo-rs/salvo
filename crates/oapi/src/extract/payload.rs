//! Request body extractors for the API operation.

pub use salvo_core::extract::{FormBody, FormFile, FormFiles, JsonBody};
use serde::Deserialize;

use crate::endpoint::EndpointArgRegister;
use crate::schema::AllOf;
use crate::{
    Array, BasicType, Components, Content, KnownFormat, Object, Operation, RefOr, RequestBody,
    Required, Schema, SchemaFormat, ToRequestBody, ToSchema,
};

// Resolve only the outer schema: nested references (including recursive types)
// remain intact. A bounded traversal also handles cyclic reference aliases.
fn resolve_form_schema(components: &Components, schema: RefOr<Schema>) -> RefOr<Schema> {
    let mut resolved = schema.clone();
    for _ in 0..=components.schemas.len() {
        let RefOr::Ref(reference) = &resolved else {
            return resolved;
        };
        if !reference.description.is_empty() || !reference.summary.is_empty() {
            return schema;
        }
        let Some(name) = reference.ref_location.strip_prefix("#/components/schemas/") else {
            return schema;
        };
        let name = name.replace("~1", "/").replace("~0", "~");
        let Some(next) = components.schemas.get(&name) else {
            return schema;
        };
        resolved = next.clone();
    }
    schema
}

fn is_field_group(object: &Object) -> bool {
    let mut shape = object.clone();
    shape.properties.clear();
    shape.required.clear();
    shape == Object::new()
}

fn merge_form_schemas(
    components: &Components,
    left: RefOr<Schema>,
    right: RefOr<Schema>,
) -> RefOr<Schema> {
    let left = resolve_form_schema(components, left);
    let right = resolve_form_schema(components, right);
    if let (RefOr::Type(Schema::Object(left)), RefOr::Type(Schema::Object(right))) = (&left, &right)
        && (is_field_group(left) || is_field_group(right))
    {
        // Add file fields to the form object itself. Using allOf with a closed
        // form object (additionalProperties: false) would reject every file.
        let (mut object, fields) = if is_field_group(right) {
            (left.clone(), right)
        } else {
            (right.clone(), left)
        };
        for (name, schema) in &fields.properties {
            let schema = match object.properties.get(name).cloned() {
                Some(existing) if existing != *schema => {
                    AllOf::new().item(existing).item(schema.clone()).into()
                }
                _ => schema.clone(),
            };
            object.properties.insert(name.clone(), schema);
        }
        object.required.extend(fields.required.iter().cloned());
        return RefOr::Type(Schema::Object(object));
    }
    // Preserve constraints and unresolved/external references for schemas that
    // cannot be flattened without changing their meaning.
    AllOf::new().item(left).item(right).into()
}

fn register_form_schema<'a>(
    components: &Components,
    operation: &'a mut Operation,
    schema: impl Into<RefOr<Schema>>,
) -> &'a mut RequestBody {
    let request_body = operation.request_body.get_or_insert_with(RequestBody::new);
    let content = request_body
        .contents
        .entry("multipart/form-data".into())
        .or_default();
    let schema = schema.into();
    content.schema = Some(match content.schema.take() {
        Some(existing) => merge_form_schemas(components, existing, schema),
        None => schema,
    });
    request_body
}

impl EndpointArgRegister for FormFile {
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let schema = Schema::from(
            Object::new()
                .property(
                    arg,
                    Object::with_type(BasicType::String)
                        .format(SchemaFormat::KnownFormat(KnownFormat::Binary)),
                )
                .required(arg),
        );
        let body = register_form_schema(components, operation, schema);
        body.required = Some(Required::True);
        body.description
            .get_or_insert_with(|| "Upload a file.".into());
    }
}

impl EndpointArgRegister for FormFiles {
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let schema = Schema::from(
            Object::new()
                .property(
                    arg,
                    Array::new().items(Schema::from(
                        Object::with_type(BasicType::String)
                            .format(SchemaFormat::KnownFormat(KnownFormat::Binary)),
                    )),
                )
                .required(arg),
        );
        let body = register_form_schema(components, operation, schema);
        body.required = Some(Required::True);
        body.description
            .get_or_insert_with(|| "Upload files.".into());
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
            .add_content("multipart/form-data", Content::new(schema))
    }
}

impl<'de, T> EndpointArgRegister for FormBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        let schema = T::to_schema(components);
        let body = register_form_schema(components, operation, schema.clone());
        body.description = Some("Extract form format data from request.".into());
        body.contents.insert(
            "application/x-www-form-urlencoded".into(),
            Content::new(schema),
        );
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
    fn multipart_file_fields_accumulate_and_remain_required() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        FormFile::register(&mut components, &mut operation, "front");
        FormFile::register(&mut components, &mut operation, "back");
        FormFiles::register(&mut components, &mut operation, "attachments");
        let body = serde_json::to_value(operation.request_body.unwrap()).unwrap();
        let schema = &body["content"]["multipart/form-data"]["schema"];
        assert_eq!(schema["properties"]["front"]["format"], "binary");
        assert_eq!(schema["properties"]["back"]["format"], "binary");
        assert_eq!(
            schema["properties"]["attachments"]["items"]["format"],
            "binary"
        );
        assert_eq!(schema["required"], json!(["front", "back", "attachments"]));
        assert_eq!(body["required"], true);
    }

    #[test]
    fn multipart_form_and_files_merge_in_either_parameter_order() {
        #[derive(Deserialize)]
        struct Fields;
        impl ToSchema for Fields {
            fn to_schema(components: &mut Components) -> RefOr<Schema> {
                components.schemas.insert(
                    "Fields",
                    Object::new()
                        .description("Text fields")
                        .property("title", Object::with_type(BasicType::String))
                        .required("title")
                        .additional_properties(crate::schema::AdditionalProperties::FreeForm(
                            false,
                        )),
                );
                crate::Ref::from_schema_name("Fields").into()
            }
        }

        for files_first in [false, true] {
            let mut components = Components::new();
            let mut operation = Operation::new();
            if files_first {
                FormFile::register(&mut components, &mut operation, "cover");
            }
            FormBody::<Fields>::register(&mut components, &mut operation, "fields");
            if !files_first {
                FormFile::register(&mut components, &mut operation, "cover");
            }
            FormFiles::register(&mut components, &mut operation, "attachments");
            let body = serde_json::to_value(operation.request_body.unwrap()).unwrap();
            let schema = &body["content"]["multipart/form-data"]["schema"];
            assert_eq!(schema["properties"]["title"]["type"], "string");
            assert_eq!(schema["properties"]["cover"]["format"], "binary");
            assert_eq!(schema["properties"]["attachments"]["type"], "array");
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(schema["description"], "Text fields");
            let required = schema["required"].as_array().unwrap();
            for name in ["title", "cover", "attachments"] {
                assert!(
                    required.contains(&json!(name)),
                    "{name}, files_first={files_first}"
                );
            }
            assert_eq!(body["required"], true);
            assert_eq!(
                body["content"]["application/x-www-form-urlencoded"]["schema"]["$ref"],
                "#/components/schemas/Fields"
            );
            // Adding files must not change the reusable text-only schema.
            let original = serde_json::to_value(&components.schemas["Fields"]).unwrap();
            assert!(original["properties"].get("cover").is_none());
        }
    }

    #[test]
    fn multipart_schema_preserves_external_and_cyclic_references() {
        let mut components = Components::new();
        components
            .schemas
            .insert("Cycle", crate::Ref::from_schema_name("Cycle"));
        for reference in [
            "https://example.com/schema.json",
            "#/components/schemas/Cycle",
        ] {
            let mut operation = Operation::new();
            operation.request_body = Some(RequestBody::new().add_content(
                "multipart/form-data",
                Content::new(crate::Ref::new(reference)),
            ));
            FormFile::register(&mut components, &mut operation, "file");
            let value = serde_json::to_value(operation.request_body.unwrap()).unwrap();
            let all_of = &value["content"]["multipart/form-data"]["schema"]["allOf"];
            assert_eq!(all_of[0]["$ref"], reference);
            assert_eq!(all_of[1]["properties"]["file"]["format"], "binary");
        }
    }

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
                    "multipart/form-data": {
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
                        "multipart/form-data": {
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
