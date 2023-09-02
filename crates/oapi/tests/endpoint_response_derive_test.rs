use assert_json_diff::assert_json_eq;
use serde_json::{json, Value};
use salvo_oapi::openapi::{RefOr, Response};
use salvo_oapi::{OpenApi, ToResponse};

mod common;

macro_rules! test_fn {
    ( module: $name:ident, responses: $($responses:tt)* ) => {
        #[allow(unused)]
        mod $name {
            #[allow(unused)]
            #[derive(salvo_oapi::ToSchema)]
            struct Foo {
                name: String,
            }

            #[salvo_oapi::endpoint(get,path = "/foo",responses $($responses)*)]
            fn get_foo() {}
        }
    };
}

macro_rules! api_doc {
    ( module: $module:expr ) => {{
        use salvo_oapi::OpenApi;
        #[derive(OpenApi, Default)]
        #[openapi(paths($module::get_foo))]
        struct ApiDoc;

        let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
        doc.pointer("/paths/~1foo/get")
            .unwrap_or(&serde_json::Value::Null)
            .clone()
    }};
}

test_fn! {
    module: simple_success_response,
    responses: (
        (status = 200, description = "success")
    )
}

#[test]
fn derive_path_with_simple_success_response() {
    let doc = api_doc!(module: simple_success_response);

    assert_value! {doc=>
        "responses.200.description" = r#""success""#, "Response description"
        "responses.200.content" = r#"null"#, "Response content"
        "responses.200.headers" = r#"null"#, "Response headers"
    }
}

test_fn! {
    module: multiple_simple_responses,
    responses: (
        (status = 200, description = "success"),
        (status = 401, description = "unauthorized"),
        (status = 404, description = "not found"),
        (status = 500, description = "server error"),
        (status = "5XX", description = "all other server errors"),
        (status = "default", description = "default")
    )
}

#[test]
fn derive_path_with_multiple_simple_responses() {
    let doc = api_doc!(module: multiple_simple_responses);

    assert_value! {doc=>
        "responses.200.description" = r#""success""#, "Response description"
        "responses.200.content" = r#"null"#, "Response content"
        "responses.200.headers" = r#"null"#, "Response headers"
        "responses.401.description" = r#""unauthorized""#, "Response description"
        "responses.401.content" = r#"null"#, "Response content"
        "responses.401.headers" = r#"null"#, "Response headers"
        "responses.404.description" = r#""not found""#, "Response description"
        "responses.404.content" = r#"null"#, "Response content"
        "responses.404.headers" = r#"null"#, "Response headers"
        "responses.500.description" = r#""server error""#, "Response description"
        "responses.500.content" = r#"null"#, "Response content"
        "responses.500.headers" = r#"null"#, "Response headers"
        "responses.5XX.description" = r#""all other server errors""#, "Response description"
        "responses.5XX.content" = r#"null"#, "Response content"
        "responses.5XX.headers" = r#"null"#, "Response headers"
        "responses.default.description" = r#""default""#, "Response description"
        "responses.default.content" = r#"null"#, "Response content"
        "responses.default.headers" = r#"null"#, "Response headers"
    }
}

test_fn! {
    module: http_status_code_responses,
    responses: (
        (status = OK, description = "success"),
        (status = http::status::StatusCode::NOT_FOUND, description = "not found"),
    )
}

#[test]
fn derive_http_status_code_responses() {
    let doc = api_doc!(module: http_status_code_responses);
    let responses = doc.pointer("/responses");

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "description": "success"
            },
            "404": {
                "description": "not found"
            }
        })
    )
}

struct ReusableResponse;

impl<'r> ToResponse<'r> for ReusableResponse {
    fn response() -> (&'r str, RefOr<Response>) {
        (
            "ReusableResponseName",
            Response::new("reusable response").into(),
        )
    }
}

test_fn! {
    module: reusable_responses,
    responses: (
        (status = 200, description = "success"),
        (status = "default", response = crate::ReusableResponse)
    )
}

#[test]
fn derive_path_with_reusable_responses() {
    let doc = api_doc!(module: reusable_responses);

    assert_value! {doc=>
        "responses.200.description" = r#""success""#, "Response description"
        "responses.default.$ref" = "\"#/components/responses/ReusableResponseName\"", "Response reference"
    }
}

macro_rules! test_response_types {
    ( $( $name:ident=> $(body: $expected:expr,)? $( $content_type:literal, )? $( headers: $headers:expr, )?
        assert: $( $path:literal = $expection:literal, $comment:literal )* )* ) => {
        $(
            paste::paste! {
                test_fn! {
                    module: [<mod_ $name>],
                    responses: (
                        (status = 200, description = "success",
                            $(body = $expected ,)*
                            $(content_type = $content_type,)*
                            $(headers $headers, )*
                        ),
                    )
                }
            }

            #[test]
            fn $name() {
                paste::paste! {
                    let doc = api_doc!(module: [<mod_ $name>]);
                }

                assert_value! {doc=>
                    "responses.200.description" = r#""success""#, "Response description"
                    $($path = $expection, $comment)*
                }
            }
        )*
    };
}

test_response_types! {
primitive_string_body => body: String, assert:
    "responses.200.content.text~1plain.schema.type" = r#""string""#, "Response content type"
    "responses.200.headers" = r###"null"###, "Response headers"
primitive_string_slice_body => body: [String], assert:
    "responses.200.content.application~1json.schema.items.type" = r#""string""#, "Response content items type"
    "responses.200.content.application~1json.schema.type" = r#""array""#, "Response content type"
    "responses.200.headers" = r###"null"###, "Response headers"
primitive_integer_slice_body => body: [i32], assert:
    "responses.200.content.application~1json.schema.items.type" = r#""integer""#, "Response content items type"
    "responses.200.content.application~1json.schema.items.format" = r#""int32""#, "Response content items format"
    "responses.200.content.application~1json.schema.type" = r#""array""#, "Response content type"
    "responses.200.headers" = r###"null"###, "Response headers"
primitive_integer_body => body: i64, assert:
    "responses.200.content.text~1plain.schema.type" = r#""integer""#, "Response content type"
    "responses.200.content.text~1plain.schema.format" = r#""int64""#, "Response content format"
    "responses.200.headers" = r###"null"###, "Response headers"
primitive_big_integer_body => body: u128, assert:
    "responses.200.content.text~1plain.schema.type" = r#""integer""#, "Response content type"
    "responses.200.content.text~1plain.schema.format" = r#"null"#, "Response content format"
    "responses.200.headers" = r###"null"###, "Response headers"
primitive_bool_body => body: bool, assert:
    "responses.200.content.text~1plain.schema.type" = r#""boolean""#, "Response content type"
    "responses.200.headers" = r###"null"###, "Response headers"
object_body => body: Foo, assert:
    "responses.200.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content type"
    "responses.200.headers" = r###"null"###, "Response headers"
object_slice_body => body: [Foo], assert:
    "responses.200.content.application~1json.schema.type" = r###""array""###, "Response content type"
    "responses.200.content.application~1json.schema.items.$ref" = r###""#/components/schemas/Foo""###, "Response content items type"
    "responses.200.headers" = r###"null"###, "Response headers"
object_body_override_content_type_to_xml => body: Foo, "text/xml", assert:
    "responses.200.content.application~1json.schema.$ref" = r###"null"###, "Response content type"
    "responses.200.content.text~1xml.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content type"
    "responses.200.headers" = r###"null"###, "Response headers"
object_body_with_simple_header => body: Foo, headers: (
    ("xsrf-token")
), assert:
    "responses.200.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content type"
    "responses.200.headers.xsrf-token.schema.type" = r###""string""###, "xsrf-token header type"
    "responses.200.headers.xsrf-token.description" = r###"null"###, "xsrf-token header description"
object_body_with_multiple_headers => body: Foo, headers: (
    ("xsrf-token"),
    ("another-header")
), assert:
    "responses.200.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content type"
    "responses.200.headers.xsrf-token.schema.type" = r###""string""###, "xsrf-token header type"
    "responses.200.headers.xsrf-token.description" = r###"null"###, "xsrf-token header description"
    "responses.200.headers.another-header.schema.type" = r###""string""###, "another-header header type"
    "responses.200.headers.another-header.description" = r###"null"###, "another-header header description"
object_body_with_header_with_type => body: Foo, headers: (
    ("random-digits" = [i64]),
), assert:
    "responses.200.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content type"
    "responses.200.headers.random-digits.schema.type" = r###""array""###, "random-digits header type"
    "responses.200.headers.random-digits.description" = r###"null"###, "random-digits header description"
    "responses.200.headers.random-digits.schema.items.type" = r###""integer""###, "random-digits header items type"
    "responses.200.headers.random-digits.schema.items.format" = r###""int64""###, "random-digits header items format"
response_no_body_with_complex_header_with_description => headers: (
    ("random-digits" = [i64], description = "Random digits response header"),
), assert:
    "responses.200.content" = r###"null"###, "Response content type"
    "responses.200.headers.random-digits.description" = r###""Random digits response header""###, "random-digits header description"
    "responses.200.headers.random-digits.schema.type" = r###""array""###, "random-digits header type"
    "responses.200.headers.random-digits.schema.items.type" = r###""integer""###, "random-digits header items type"
    "responses.200.headers.random-digits.schema.items.format" = r###""int64""###, "random-digits header items format"
binary_octet_stream => body: [u8], assert:
    "responses.200.content.application~1octet-stream.schema.type" = r#""string""#, "Response content type"
    "responses.200.content.application~1octet-stream.schema.format" = r#""binary""#, "Response content format"
    "responses.200.headers" = r###"null"###, "Response headers"
}

test_fn! {
    module: response_with_json_example,
    responses: (
        (status = 200, description = "success", body = Foo, example = json!({"foo": "bar"}))
    )
}

#[test]
fn derive_response_with_json_example_success() {
    let doc = api_doc!(module: response_with_json_example);

    assert_value! {doc=>
        "responses.200.description" = r#""success""#, "Response description"
        "responses.200.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content ref"
        "responses.200.content.application~1json.example" = r###"{"foo":"bar"}"###, "Response content example"
        "responses.200.headers" = r#"null"#, "Response headers"
    }
}

#[test]
fn derive_response_multiple_content_types() {
    test_fn! {
        module: response_multiple_content_types,
        responses: (
            (status = 200, description = "success", body = Foo, content_type = ["text/xml", "application/json"])
        )
    }

    let doc = api_doc!(module: response_multiple_content_types);

    assert_value! {doc=>
        "responses.200.description" = r#""success""#, "Response description"
        "responses.200.content.application~1json.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content ref"
        "responses.200.content.text~1xml.schema.$ref" = r###""#/components/schemas/Foo""###, "Response content ref"
        "responses.200.content.application~1json.example" = r###"null"###, "Response content example"
        "responses.200.content.text~1xml.example" = r###"null"###, "Response content example"
        "responses.200.headers" = r#"null"#, "Response headers"
    }
}

#[test]
fn derive_response_body_inline_schema_component() {
    test_fn! {
        module: response_body_inline_schema,
        responses: (
            (status = 200, description = "success", body = inline(Foo), content_type = ["application/json"])
        )
    }

    let doc: Value = api_doc!(module: response_body_inline_schema);

    assert_json_eq!(
        doc,
        json!({
            "operationId": "get_foo",
            "responses": {
                "200": {
                    "content": {
                        "application/json": {
                            "schema": {
                                "properties": {
                                    "name": {
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
                    "description": "success"
                }
            },
            "tags": [
              "response_body_inline_schema"
            ]
        })
    );
}

#[test]
fn derive_path_with_multiple_responses_via_content_attribute() {
    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct User {
        id: String,
    }

    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct User2 {
        id: i32,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/foo", 
        responses(
            (status = 200, content(
                    ("application/vnd.user.v1+json" = User, example = json!(User {id: "id".to_string()})),
                    ("application/vnd.user.v2+json" = User2, example = json!(User2 {id: 2}))
                )
            )
        )
    )]
    #[allow(unused)]
    fn get_item() {}

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(paths(get_item), components(schemas(User, User2)))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let responses = doc.pointer("/paths/~1foo/get/responses").unwrap();

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/vnd.user.v1+json": {
                        "example": {
                            "id": "id",
                        },
                        "schema": {
                            "$ref":
                                "#/components/schemas/User",
                        },
                    },
                    "application/vnd.user.v2+json": {
                        "example": {
                            "id": 2,
                        },
                        "schema": {
                            "$ref": "#/components/schemas/User2",
                        },
                    },
                },
                "description": "",
            },
        })
    )
}

#[test]
fn derive_path_with_multiple_examples() {
    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct User {
        name: String,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/foo", 
        responses(
            (status = 200, body = User,
                examples(
                    ("Demo" = (summary = "This is summary", description = "Long description", 
                                value = json!(User{name: "Demo".to_string()}))),
                    ("John" = (summary = "Another user", value = json!({"name": "John"})))
                 )
            )
        )
    )]
    #[allow(unused)]
    fn get_item() {}

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(paths(get_item), components(schemas(User)))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let responses = doc.pointer("/paths/~1foo/get/responses").unwrap();

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/json": {
                        "examples": {
                            "Demo": {
                                "summary": "This is summary",
                                "description": "Long description",
                                "value": {
                                    "name": "Demo"
                                }
                            },
                            "John": {
                                "summary": "Another user",
                                "value": {
                                    "name": "John"
                                }
                            }
                        },
                        "schema": {
                            "$ref":
                                "#/components/schemas/User",
                        },
                    },
                },
                "description": "",
            },
        })
    )
}

#[test]
fn derive_path_with_multiple_responses_with_multiple_examples() {
    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct User {
        id: String,
    }

    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct User2 {
        id: i32,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/foo", 
        responses(
            (status = 200, content(
                    ("application/vnd.user.v1+json" = User, 
                        examples(
                            ("StringUser" = (value = json!({"id": "1"}))),
                            ("StringUser2" = (value = json!({"id": "2"})))
                        ),
                    ),
                    ("application/vnd.user.v2+json" = User2, 
                        examples(
                            ("IntUser" = (value = json!({"id": 1}))),
                            ("IntUser2" = (value = json!({"id": 2})))
                        ),
                    )
                )
            )
        )
    )]
    #[allow(unused)]
    fn get_item() {}

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(paths(get_item), components(schemas(User, User2)))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let responses = doc.pointer("/paths/~1foo/get/responses").unwrap();

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/vnd.user.v1+json": {
                        "examples": {
                            "StringUser": {
                                "value": {
                                    "id": "1"
                                }
                            },
                            "StringUser2": {
                                "value": {
                                    "id": "2"
                                }
                            }
                        },
                        "schema": {
                            "$ref":
                                "#/components/schemas/User",
                        },
                    },
                    "application/vnd.user.v2+json": {
                        "examples": {
                            "IntUser": {
                                "value": {
                                    "id": 1
                                }
                            },
                            "IntUser2": {
                                "value": {
                                    "id": 2
                                }
                            }
                        },
                        "schema": {
                            "$ref": "#/components/schemas/User2",
                        },
                    },
                },
                "description": "",
            },
        })
    )
}

#[test]
fn path_response_with_external_ref() {
    #[salvo_oapi::endpoint(
        get,
        path = "/foo", 
        responses(
            (status = 200, body = ref("./MyUser.json"))
        )
    )]
    #[allow(unused)]
    fn get_item() {}

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(paths(get_item))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let responses = doc.pointer("/paths/~1foo/get/responses").unwrap();

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref":
                                "./MyUser.json",
                        },
                    },
                },
                "description": "",
            },
        })
    )
}

#[test]
fn path_response_with_inline_ref_type() {
    #[derive(serde::Serialize, salvo_oapi::ToSchema, salvo_oapi::ToResponse)]
    #[allow(unused)]
    struct User {
        name: String,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/foo", 
        responses(
            (status = 200, response = inline(User))
        )
    )]
    #[allow(unused)]
    fn get_item() {}

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(paths(get_item), components(schemas(User)))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let responses = doc.pointer("/paths/~1foo/get/responses").unwrap();

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/json": {
                        "schema": {
                            "properties": {
                                "name": {
                                    "type": "string"
                                }
                            },
                            "required": ["name"],
                            "type": "object",
                        },
                    },
                },
                "description": "",
            },
        })
    )
}
