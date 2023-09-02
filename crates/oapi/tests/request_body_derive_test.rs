use assert_json_diff::assert_json_eq;
use serde_json::{json, Value};
use salvo_oapi::OpenApi;
use salvo_oapi_gen::ToSchema;

mod common;

macro_rules! test_fn {
    ( module: $name:ident, body: $($body:tt)* ) => {
        #[allow(unused)]
        mod $name {
            #[derive(salvo_oapi::ToSchema)]
            /// Some struct
            pub struct Foo {
                /// Some name
                name: String,
            }
            #[salvo_oapi::endpoint(
                post,
                path = "/foo",
                request_body $($body)*,
                responses(
                    (status = 200, description = "success response")
                )
            )]
            fn post_foo() {}
        }
    };
}

test_fn! {
    module: derive_request_body_simple,
    body: = Foo
}

#[test]
fn derive_path_request_body_simple_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_simple::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    assert_value! {doc=>
        "paths.~1foo.post.requestBody.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Request body content object type"
        "paths.~1foo.post.requestBody.content.text~1plain" = r###"null"###, "Request body content object type not text/plain"
        "paths.~1foo.post.requestBody.required" = r###"true"###, "Request body required"
        "paths.~1foo.post.requestBody.description" = r###"null"###, "Request body description"
    }
}

test_fn! {
    module: derive_request_body_simple_array,
    body: = [Foo]
}

#[test]
fn derive_path_request_body_simple_array_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_simple_array::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    assert_value! {doc=>
        "paths.~1foo.post.requestBody.content.application~1json.schema.$ref" = r###"null"###, "Request body content object type"
        "paths.~1foo.post.requestBody.content.application~1json.schema.items.$ref" = r###""#/components/schemas/Foo""###, "Request body content items object type"
        "paths.~1foo.post.requestBody.content.application~1json.schema.type" = r###""array""###, "Request body content items type"
        "paths.~1foo.post.requestBody.content.text~1plain" = r###"null"###, "Request body content object type not text/plain"
        "paths.~1foo.post.requestBody.required" = r###"true"###, "Request body required"
        "paths.~1foo.post.requestBody.description" = r###"null"###, "Request body description"
    }
}

test_fn! {
    module: derive_request_body_option_array,
    body: = Option<[Foo]>
}

#[test]
fn derive_request_body_option_array_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_option_array::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let body = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        body,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "items": {
                            "$ref": "#/components/schemas/Foo"
                        },
                        "nullable": true,
                        "type": "array",
                    },
                }
            },
            "required": false,
        })
    );
}

test_fn! {
    module: derive_request_body_primitive_simple,
    body: = String
}

#[test]
fn derive_request_body_primitive_simple_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_primitive_simple::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    assert_value! {doc=>
        "paths.~1foo.post.requestBody.content.application~1json.schema.$ref" = r###"null"###, "Request body content object type not application/json"
        "paths.~1foo.post.requestBody.content.application~1json.schema.items.$ref" = r###"null"###, "Request body content items object type"
        "paths.~1foo.post.requestBody.content.application~1json.schema.type" = r###"null"###, "Request body content items type"
        "paths.~1foo.post.requestBody.content.text~1plain.schema.type" = r###""string""###, "Request body content object type"
        "paths.~1foo.post.requestBody.required" = r###"true"###, "Request body required"
        "paths.~1foo.post.requestBody.description" = r###"null"###, "Request body description"
    }
}

test_fn! {
    module: derive_request_body_primitive_simple_array,
    body: = [i64]
}

#[test]
fn derive_request_body_primitive_array_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_primitive_simple_array::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let content = doc
        .pointer("/paths/~1foo/post/requestBody/content")
        .unwrap();

    assert_json_eq!(
        content,
        json!(
        {
            "application/json": {
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "integer",
                        "format": "int64",
                    }
                }
            }
        }
        )
    );
}

test_fn! {
    module: derive_request_body_complex,
    body: (content = Foo, description = "Create new Foo", content_type = "text/xml")
}

#[test]
fn derive_request_body_complex_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_complex::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let request_body: &Value = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        request_body,
        json!({
            "content": {
                "text/xml": {
                    "schema": {
                        "$ref": "#/components/schemas/Foo"
                    }
                }
            },
            "description": "Create new Foo",
            "required": true
        })
    );
}

test_fn! {
    module: derive_request_body_complex_inline,
    body: (content = inline(Foo), description = "Create new Foo", content_type = "text/xml")
}

#[test]
fn derive_request_body_complex_success_inline() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_complex_inline::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let request_body: &Value = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        request_body,
        json!({
            "content": {
                "text/xml": {
                    "schema": {
                        "description": "Some struct",
                        "properties": {
                            "name": {
                                "description": "Some name",
                                "type": "string"
                            }
                        },
                        "required": [
                            "name"
                        ],
                        "type": "object"
                    }
                }
            },
            "description": "Create new Foo",
            "required": true
        })
    );
}

test_fn! {
    module: derive_request_body_complex_array,
    body: (content = [Foo], description = "Create new Foo", content_type = "text/xml")
}

#[test]
fn derive_request_body_complex_success_array() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_complex_array::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let request_body: &Value = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        request_body,
        json!({
            "content": {
                "text/xml": {
                    "schema": {
                        "items": {
                            "$ref": "#/components/schemas/Foo"
                        },
                        "type": "array"
                    }
                }
            },
            "description": "Create new Foo",
            "required": true
        })
    );
}

test_fn! {
    module: derive_request_body_complex_inline_array,
    body: (content = inline([Foo]), description = "Create new Foo", content_type = "text/xml")
}

#[test]
fn derive_request_body_complex_success_inline_array() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_complex_inline_array::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let request_body: &Value = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        request_body,
        json!({
            "content": {
                "text/xml": {
                    "schema": {
                        "items": {
                            "description": "Some struct",
                            "properties": {
                                "name": {
                                    "description": "Some name",
                                    "type": "string"
                                }
                            },
                            "required": [
                                "name"
                            ],
                            "type": "object"
                        },
                        "type": "array"
                    }
                }
            },
            "description": "Create new Foo",
            "required": true
        })
    );
}

test_fn! {
    module: derive_request_body_simple_inline,
    body: = inline(Foo)
}

#[test]
fn derive_request_body_simple_inline_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_simple_inline::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let request_body: &Value = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        request_body,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "description": "Some struct",
                        "properties": {
                            "name": {
                                "description": "Some name",
                                "type": "string"
                            }
                        },
                        "required": [
                            "name"
                        ],
                        "type": "object"
                    }
                }
            },
            "required": true
        })
    );
}

test_fn! {
    module: derive_request_body_complex_required_explicit,
    body: (content = Option<Foo>, description = "Create new Foo", content_type = "text/xml")
}

#[test]
fn derive_request_body_complex_required_explicit_false_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_complex_required_explicit::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let body = doc.pointer("/paths/~1foo/post/requestBody").unwrap();

    assert_json_eq!(
        body,
        json!({
            "content": {
                "text/xml": {
                    "schema": {
                        "allOf": [
                        {
                            "$ref": "#/components/schemas/Foo"
                        }
                        ],
                        "nullable": true,
                    }
                }
            },
            "description": "Create new Foo",
            "required": false,
        })
    );
}

test_fn! {
    module: derive_request_body_complex_primitive_array,
    body: (content = [i32], description = "Create new foo references")
}

#[test]
fn derive_request_body_complex_primitive_array_success() {
    #[derive(OpenApi, Default)]
    #[openapi(paths(derive_request_body_complex_primitive_array::post_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let content = doc
        .pointer("/paths/~1foo/post/requestBody/content")
        .unwrap();
    assert_json_eq!(
        content,
        json!(
        {
            "application/json": {
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "integer",
                        "format": "int32",
                    }
                }
            }
        }
        )
    );
}

test_fn! {
    module: derive_request_body_ref_path,
    body: = path::to::Foo
}

#[test]
fn derive_request_body_ref_path_success() {
    /// Some struct
    #[derive(ToSchema)]
    #[schema(as = path::to::Foo)]
    #[allow(unused)]
    pub struct Foo {
        /// Some name
        name: String,
    }

    #[derive(OpenApi, Default)]
    #[openapi(
        paths(derive_request_body_ref_path::post_foo),
        components(schemas(Foo))
    )]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let schemas = doc.pointer("/components/schemas").unwrap();
    assert!(schemas.get("path.to.Foo").is_some());

    let component_ref: &str = doc
        .pointer("/paths/~1foo/post/requestBody/content/application~1json/schema/$ref")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(component_ref, "#/components/schemas/path.to.Foo");
}

#[test]
fn unit_type_request_body() {
    #[salvo_oapi::endpoint(
        post,
        path = "/unit_type_test",
        request_body = ()
    )]
    #[allow(unused)]
    fn unit_type_test() {}

    #[derive(OpenApi)]
    #[openapi(paths(unit_type_test))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let request_body = doc
        .pointer("/paths/~1unit_type_test/post/requestBody")
        .unwrap();

    assert_json_eq!(
        request_body,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "default": null,
                        "nullable": true,
                    }
                }
            },
            "required": true
        })
    )
}

#[test]
fn request_body_with_example() {
    #[derive(ToSchema)]
    #[allow(unused)]
    struct Foo<'v> {
        value: &'v str,
    }

    #[salvo_oapi::endpoint(get, path = "/item", request_body(content = Foo, example = json!({"value": "this is value"})))]
    #[allow(dead_code)]
    fn get_item() {}

    #[derive(OpenApi)]
    #[openapi(components(schemas(Foo)), paths(get_item))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let content = doc
        .pointer("/paths/~1item/get/requestBody/content")
        .unwrap();
    assert_json_eq!(
        content,
        json!(
            {"application/json": {
                "example": {
                    "value": "this is value"
                },
                "schema": {
                    "$ref": "#/components/schemas/Foo"
                }
            }
        })
    )
}

#[test]
fn request_body_with_examples() {
    #[derive(ToSchema)]
    #[allow(unused)]
    struct Foo<'v> {
        value: &'v str,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/item",
        request_body(content = Foo,
            examples(
                ("Value1" = (value = json!({"value": "this is value"}) ) ),
                ("Value2" = (value = json!({"value": "this is value2"}) ) )
            )
        )
    )]
    #[allow(dead_code)]
    fn get_item() {}

    #[derive(OpenApi)]
    #[openapi(components(schemas(Foo)), paths(get_item))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let content = doc
        .pointer("/paths/~1item/get/requestBody/content")
        .unwrap();
    assert_json_eq!(
        content,
        json!(
            {"application/json": {
                "examples": {
                    "Value1": {
                        "value": {
                            "value": "this is value"
                        }
                    },
                    "Value2": {
                        "value": {
                            "value": "this is value2"
                        }
                    }
                },
                "schema": {
                    "$ref": "#/components/schemas/Foo"
                }
            }
        })
    )
}

#[test]
fn request_body_with_binary() {
    #[salvo_oapi::endpoint(get, path = "/item", request_body(content = [u8]))]
    #[allow(dead_code)]
    fn get_item() {}

    #[derive(OpenApi)]
    #[openapi(paths(get_item))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let content = doc
        .pointer("/paths/~1item/get/requestBody/content")
        .unwrap();
    assert_json_eq!(
        content,
        json!(
            {"application/octet-stream": {
                "schema": {
                    "type": "string",
                    "format": "binary"
                }
            }
        })
    )
}

#[test]
fn request_body_with_external_ref() {
    #[salvo_oapi::endpoint(get, path = "/item", request_body(content = ref("./MyUser.json")))]
    #[allow(dead_code)]
    fn get_item() {}

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(paths(get_item))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();

    let content = doc
        .pointer("/paths/~1item/get/requestBody/content")
        .unwrap();
    assert_json_eq!(
        content,
        json!(
            {"application/json": {
                "schema": {
                    "$ref": "./MyUser.json"
                }
            }
        })
    )
}
