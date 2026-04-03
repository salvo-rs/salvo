#![allow(missing_docs, clippy::unwrap_used)]

use assert_json_diff::assert_json_eq;
use salvo::oapi::extract::*;
use salvo::oapi::{Components, ComposeSchema, RefOr};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Test 1: ComposeSchema for a simple generic struct with one type param
#[test]
fn test_compose_schema_simple_generic() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Wrapper<T: ToSchema + ComposeSchema + 'static> {
        inner: T,
    }

    #[endpoint]
    async fn use_wrapper(body: JsonBody<Wrapper<String>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(use_wrapper));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    assert_json_eq!(
        schemas["Wrapper_String_"],
        json!({
            "type": "object",
            "required": ["inner"],
            "properties": {
                "inner": { "type": "string" }
            }
        })
    );
}

/// Test 2: ComposeSchema for generic struct with Vec<T> field (nested generic)
#[test]
fn test_compose_schema_vec_of_generic() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Page<T: ToSchema + ComposeSchema + 'static> {
        items: Vec<T>,
        total: u64,
    }

    #[endpoint]
    async fn list_strings(body: JsonBody<Page<String>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(list_strings));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    assert_json_eq!(
        schemas["Page_String_"],
        json!({
            "type": "object",
            "required": ["items", "total"],
            "properties": {
                "items": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "total": {
                    "type": "integer",
                    "format": "uint64",
                    "minimum": 0
                }
            }
        })
    );
}

/// Test 3: Multiple generic params
#[test]
fn test_compose_schema_multiple_generic_params() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Pair<A: ToSchema + ComposeSchema + 'static, B: ToSchema + ComposeSchema + 'static> {
        first: A,
        second: B,
    }

    #[endpoint]
    async fn use_pair(body: JsonBody<Pair<String, i32>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(use_pair));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    assert_json_eq!(
        schemas["Pair_String, i32_"],
        json!({
            "type": "object",
            "required": ["first", "second"],
            "properties": {
                "first": { "type": "string" },
                "second": {
                    "type": "integer",
                    "format": "int32"
                }
            }
        })
    );
}

/// Test 4: Generic struct used as field type in another generic struct (nested user generics)
#[test]
fn test_compose_schema_nested_user_generics() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Inner<T: ToSchema + ComposeSchema + 'static> {
        data: T,
    }

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Outer<T: ToSchema + ComposeSchema + 'static> {
        wrapped: Inner<T>,
        label: String,
    }

    #[endpoint]
    async fn use_nested(body: JsonBody<Outer<i64>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(use_nested));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    // Inner<i64> should be registered as a separate schema
    assert_json_eq!(
        schemas["Inner_i64_"],
        json!({
            "type": "object",
            "required": ["data"],
            "properties": {
                "data": {
                    "type": "integer",
                    "format": "int64"
                }
            }
        })
    );

    // Outer<i64> should reference Inner<i64> via $ref
    assert_json_eq!(
        schemas["Outer_i64_"],
        json!({
            "type": "object",
            "required": ["wrapped", "label"],
            "properties": {
                "wrapped": { "$ref": "#/components/schemas/Inner_i64_" },
                "label": { "type": "string" }
            }
        })
    );
}

/// Test 5: Generic with Option<T> field (nullable generic param)
#[test]
fn test_compose_schema_option_generic() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct MaybeValue<T: ToSchema + ComposeSchema + 'static> {
        value: Option<T>,
        name: String,
    }

    #[endpoint]
    async fn use_maybe(body: JsonBody<MaybeValue<f64>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(use_maybe));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    let schema = &schemas["MaybeValue_f64_"];
    // value should be optional (not in required) and nullable
    let required = schema["required"].as_array().unwrap();
    let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(required_names.contains(&"name"), "name should be required");
    assert!(
        !required_names.contains(&"value"),
        "value should NOT be required (it's Option)"
    );
    // value property should have the f64 schema with null type
    assert!(schema["properties"]["value"].is_object());
}

/// Test 6: Generic with HashMap<String, T> field
#[test]
fn test_compose_schema_hashmap_generic() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Registry<T: ToSchema + ComposeSchema + 'static> {
        entries: std::collections::HashMap<String, T>,
    }

    #[endpoint]
    async fn use_registry(body: JsonBody<Registry<bool>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(use_registry));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    let schema = &schemas["Registry_bool_"];
    // entries should be an object with additionalProperties
    assert_json_eq!(
        schema["properties"]["entries"],
        json!({
            "type": "object",
            "additionalProperties": { "type": "boolean" },
            "propertyNames": { "type": "string" }
        })
    );
}

/// Test 7: Generic struct with mixed generic and concrete fields
#[test]
fn test_compose_schema_mixed_fields() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct ApiResponse<T: ToSchema + ComposeSchema + 'static> {
        status: String,
        code: u16,
        data: T,
        tags: Vec<String>,
    }

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct User {
        name: String,
        age: u32,
    }

    #[endpoint]
    async fn get_user(_body: JsonBody<ApiResponse<User>>) -> String {
        String::new()
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("test").post(get_user));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    // User should be a separate schema
    assert!(schemas["User"].is_object(), "User schema should exist");
    assert_json_eq!(
        schemas["User"]["properties"],
        json!({
            "name": { "type": "string" },
            "age": { "type": "integer", "format": "uint32", "minimum": 0 }
        })
    );

    // ApiResponse<User> should reference User via $ref
    let api_resp = &schemas["ApiResponse_User_"];
    assert_json_eq!(
        api_resp["properties"]["status"],
        json!({ "type": "string" })
    );
    assert_json_eq!(
        api_resp["properties"]["code"],
        json!({ "type": "integer", "format": "uint16", "minimum": 0 })
    );
    assert_json_eq!(
        api_resp["properties"]["data"],
        json!({ "$ref": "#/components/schemas/User" })
    );
    assert_json_eq!(
        api_resp["properties"]["tags"],
        json!({ "type": "array", "items": { "type": "string" } })
    );
}

/// Test 8: Direct ComposeSchema::compose call (programmatic usage)
#[test]
fn test_compose_schema_direct_call() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Container<T: ToSchema + ComposeSchema + 'static> {
        item: T,
    }

    // Call ComposeSchema::compose directly with a manually-built schema
    let mut components = Components::default();
    let custom_schema: RefOr<salvo::oapi::schema::Schema> = salvo::oapi::Object::new()
        .schema_type(salvo::oapi::BasicType::String)
        .into();

    let composed =
        <Container<String> as ComposeSchema>::compose(&mut components, vec![custom_schema]);

    // The composed schema should use the provided generic schema
    let value = serde_json::to_value(&composed).unwrap();
    assert_json_eq!(
        value,
        json!({
            "type": "object",
            "required": ["item"],
            "properties": {
                "item": { "type": "string" }
            }
        })
    );
}

/// Test 9: SchemaReference helper methods
#[test]
fn test_schema_reference_helpers() {
    use salvo::oapi::SchemaReference;

    let reference = SchemaReference::new("Response")
        .reference(SchemaReference::new("Vec").reference(SchemaReference::new("User")))
        .reference(SchemaReference::new("String"));

    assert_eq!(reference.compose_name(), "Response<Vec<User>, String>");

    let generics = reference.compose_generics();
    assert_eq!(generics.len(), 2);
    assert_eq!(generics[0].name, "Vec");
    assert_eq!(generics[1].name, "String");

    let all_children = reference.compose_child_references();
    assert_eq!(all_children.len(), 3); // Vec, User, String
    assert_eq!(all_children[0].name, "Vec");
    assert_eq!(all_children[1].name, "User"); // depth-first under Vec
    assert_eq!(all_children[2].name, "String");
}

/// Test 10: Same generic struct instantiated with different type params
#[test]
fn test_compose_schema_multiple_instantiations() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct Box2<T: ToSchema + ComposeSchema + 'static> {
        content: T,
    }

    #[endpoint]
    async fn use_box_string(body: JsonBody<Box2<String>>) -> String {
        format!("{body:?}")
    }
    #[endpoint]
    async fn use_box_i32(body: JsonBody<Box2<i32>>) -> String {
        format!("{body:?}")
    }
    #[endpoint]
    async fn use_box_bool(body: JsonBody<Box2<bool>>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new()
        .push(Router::with_path("string").post(use_box_string))
        .push(Router::with_path("i32").post(use_box_i32))
        .push(Router::with_path("bool").post(use_box_bool));
    let doc = OpenApi::new("test", "0.1.0").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();

    // All three instantiations should produce distinct schemas
    assert_json_eq!(
        schemas["Box2_String_"]["properties"]["content"],
        json!({ "type": "string" })
    );
    assert_json_eq!(
        schemas["Box2_i32_"]["properties"]["content"],
        json!({ "type": "integer", "format": "int32" })
    );
    assert_json_eq!(
        schemas["Box2_bool_"]["properties"]["content"],
        json!({ "type": "boolean" })
    );
}
