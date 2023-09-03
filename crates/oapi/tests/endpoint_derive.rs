use std::collections::BTreeMap;

use assert_json_diff::{assert_json_eq, assert_json_matches, CompareMode, Config, NumericMode};
use paste::paste;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use salvo_oapi::openapi::RefOr;
use salvo_oapi::openapi::{Object, ObjectBuilder};
use salvo_oapi::{
    openapi::{Response, ResponseBuilder, ResponsesBuilder},
    ToParameters, IntoResponses, OpenApi, ToSchema,
};

mod common;

#[test]
fn derive_path_with_all_info_success() {
    let operation = test_api_fn_doc! {
        derive_path_with_all_info::test_operation2,
        operation: post,
        path: "/foo/bar/{id}"
    };

    let router = Router::with_path("/foo/bar/<id>").post(salvo::handler::empty());

    common::assert_json_array_len(operation.pointer("/parameters").unwrap(), 1);
    assert_value! {operation=>
       "deprecated" = r#"true"#, "Api fn deprecated status"
       "description" = r#""This is test operation description\n\nAdditional info in long description""#, "Api fn description"
       "summary" = r#""This is test operation description""#, "Api fn summary"
       "operationId" = r#""foo_bar_id""#, "Api fn operation_id"
       "tags.[0]" = r#""custom_tag""#, "Api fn tag"

       "parameters.[0].deprecated" = r#"null"#, "Path parameter deprecated"
       "parameters.[0].description" = r#""Foo bar id""#, "Path parameter description"
       "parameters.[0].in" = r#""path""#, "Path parameter in"
       "parameters.[0].name" = r#""id""#, "Path parameter name"
       "parameters.[0].required" = r#"true"#, "Path parameter required"
    }
}

#[test]
fn derive_path_with_defaults_success() {
    test_api_fn! {
        name: test_operation3,
        module: derive_path_with_defaults,
        operation: post,
        path: "/foo/bar";
    }
    let operation = test_api_fn_doc! {
        derive_path_with_defaults::test_operation3,
        operation: post,
        path: "/foo/bar"
    };

    assert_value! {operation=>
       "deprecated" = r#"null"#, "Api fn deprecated status"
       "operationId" = r#""test_operation3""#, "Api fn operation_id"
       "tags.[0]" = r#""derive_path_with_defaults""#, "Api fn tag"
       "parameters" = r#"null"#, "Api parameters"
    }
}

#[test]
fn derive_path_with_extra_attributes_without_nested_module() {
    /// This is test operation
    ///
    /// This is long description for test operation
    #[salvo_oapi::endpoint(
        get,
        path = "/foo/{id}",
        responses(
            (
                status = 200, description = "success response")
            ),
            parameters(
                ("id" = i64, deprecated = false, description = "Foo database id"),
                ("since" = Option<String>, Query, deprecated = false, description = "Datetime since foo is updated")
            )
    )]
    #[allow(unused)]
    async fn get_foos_by_id_since() -> String {
        "".to_string()
    }

    let operation = test_api_fn_doc! {
        get_foos_by_id_since,
        operation: get,
        path: "/foo/{id}"
    };

    common::assert_json_array_len(operation.pointer("/parameters").unwrap(), 2);
    assert_value! {operation=>
        "deprecated" = r#"null"#, "Api operation deprecated"
        "description" = r#""This is test operation\n\nThis is long description for test operation""#, "Api operation description"
        "operationId" = r#""get_foos_by_id_since""#, "Api operation operation_id"
        "summary" = r#""This is test operation""#, "Api operation summary"
        "tags.[0]" = r#""crate""#, "Api operation tag"

        "parameters.[0].deprecated" = r#"false"#, "Parameter 0 deprecated"
        "parameters.[0].description" = r#""Foo database id""#, "Parameter 0 description"
        "parameters.[0].in" = r#""path""#, "Parameter 0 in"
        "parameters.[0].name" = r#""id""#, "Parameter 0 name"
        "parameters.[0].required" = r#"true"#, "Parameter 0 required"
        "parameters.[0].schema.format" = r#""int64""#, "Parameter 0 schema format"
        "parameters.[0].schema.type" = r#""integer""#, "Parameter 0 schema type"

        "parameters.[1].deprecated" = r#"false"#, "Parameter 1 deprecated"
        "parameters.[1].description" = r#""Datetime since foo is updated""#, "Parameter 1 description"
        "parameters.[1].in" = r#""query""#, "Parameter 1 in"
        "parameters.[1].name" = r#""since""#, "Parameter 1 name"
        "parameters.[1].required" = r#"false"#, "Parameter 1 required"
        "parameters.[1].schema.allOf.[0].format" = r#"null"#, "Parameter 1 schema format"
        "parameters.[1].schema.allOf.[0].type" = r#"null"#, "Parameter 1 schema type"
        "parameters.[1].schema.allOf.nullable" = r#"null"#, "Parameter 1 schema type"
    }
}

#[test]
fn derive_path_with_security_requirements() {
    #[salvo_oapi::endpoint(
        get,
        path = "/items",
        responses(
            (status = 200, description = "success response")
        ),
        security(
            (),
            ("api_oauth" = ["read:items", "edit:items"]),
            ("jwt_token" = [])
        )
    )]
    #[allow(unused)]
    fn get_items() -> String {
        "".to_string()
    }
    let operation = test_api_fn_doc! {
        get_items,
        operation: get,
        path: "/items"
    };

    assert_value! {operation=>
        "security.[0]" = "{}", "Optional security requirement"
        "security.[1].api_oauth.[0]" = r###""read:items""###, "api_oauth first scope"
        "security.[1].api_oauth.[1]" = r###""edit:items""###, "api_oauth second scope"
        "security.[2].jwt_token" = "[]", "jwt_token auth scopes"
    }
}

#[test]
fn derive_path_with_parameter_schema() {
    #[derive(serde::Deserialize, salvo_oapi::ToSchema)]
    struct Since {
        /// Some date
        #[allow(dead_code)]
        date: String,
        /// Some time
        #[allow(dead_code)]
        time: String,
    }

    /// This is test operation
    ///
    /// This is long description for test operation
    #[salvo_oapi::endpoint(
        get,
        path = "/foo/{id}",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            ("id" = i64, description = "Foo database id"),
            ("since" = Option<Since>, Query, description = "Datetime since foo is updated")
        )
    )]
    #[allow(unused)]
    async fn get_foos_by_id_since() -> String {
        "".to_string()
    }

    let operation: Value = test_api_fn_doc! {
        get_foos_by_id_since,
        operation: get,
        path: "/foo/{id}"
    };

    let parameters: &Value = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "description": "Foo database id",
                "in": "path",
                "name": "id",
                "required": true,
                "schema": {
                    "format": "int64",
                    "type": "integer",
                }
            },
            {
                "description": "Datetime since foo is updated",
                "in": "query",
                "name": "since",
                "required": false,
                "schema": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/Since"
                        }
                    ],
                    "nullable": true,
                }
            }
        ])
    );
}

#[test]
fn derive_path_with_parameter_inline_schema() {
    #[derive(serde::Deserialize, salvo_oapi::ToSchema)]
    struct Since {
        /// Some date
        #[allow(dead_code)]
        date: String,
        /// Some time
        #[allow(dead_code)]
        time: String,
    }

    /// This is test operation
    ///
    /// This is long description for test operation
    #[salvo_oapi::endpoint(
        path = "/foo/{id}",
        responses(
            (status = 200, description = "success response")
        ),
        paramters(
            ("id" = i64, description = "Foo database id"),
            ("since" = inline(Option<Since>), Query, description = "Datetime since foo is updated")
        )
    )]
    #[allow(unused)]
    async fn get_foos_by_id_since() -> String {
        "".to_string()
    }

    let operation: Value = test_api_fn_doc! {
        get_foos_by_id_since,
        operation: get,
        path: "/foo/{id}"
    };

    let parameters: &Value = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "description": "Foo database id",
                "in": "path",
                "name": "id",
                "required": true,
                "schema": {
                    "format": "int64",
                    "type": "integer",
                }
            },
            {
                "description": "Datetime since foo is updated",
                "in": "query",
                "name": "since",
                "required": false,
                "schema": {
                    "allOf": [
                        {
                            "properties": {
                                "date": {
                                    "description": "Some date",
                                    "type": "string"
                                },
                                "time": {
                                    "description": "Some time",
                                    "type": "string"
                                }
                            },
                            "required": [
                                "date",
                                "time"
                            ],
                            "type": "object"
                        }
                    ],
                    "nullable": true,
                }
            }
        ])
    );
}

#[test]
fn derive_path_params_map() {
    #[derive(serde::Deserialize, ToSchema)]
    enum Foo {
        Bar,
        Baz,
    }

    #[derive(serde::Deserialize, ToParameters)]
    #[allow(unused)]
    struct MyParams {
        with_ref: HashMap<String, Foo>,
        with_type: HashMap<String, String>,
    }

    #[salvo_oapi::endpoint(
        path = "/foo",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            MyParams,
        )
    )]
    #[allow(unused)]
    fn use_maps(params: MyParams) -> String {
        "".to_string()
    }

    let operation: Value = test_api_fn_doc! {
        use_maps,
        operation: get,
        path: "/foo"
    };

    let parameters = operation.get("parameters").unwrap();

    assert_json_eq! {
        parameters,
        json!{[
            {
            "in": "path",
            "name": "with_ref",
            "required": true,
            "schema": {
              "additionalProperties": {
                "$ref": "#/components/schemas/Foo"
              },
              "type": "object"
            }
          },
          {
            "in": "path",
            "name": "with_type",
            "required": true,
            "schema": {
              "additionalProperties": {
                "type": "string"
              },
              "type": "object"
            }
          }
        ]}
    }
}

#[test]
fn derive_path_params_with_examples() {
    let operation = api_fn_doc_with_params! {get: "/foo" =>
        struct MyParams {
            #[param(example = json!({"key": "value"}))]
            map: HashMap<String, String>,
            #[param(example = json!(["value1", "value2"]))]
            vec: Vec<String>,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq! {
        parameters,
        json!{[
            {
            "in": "path",
            "name": "map",
            "required": true,
            "example": {
                "key": "value"
            },
            "schema": {
              "additionalProperties": {
                "type": "string"
              },
              "type": "object"
            }
          },
          {
            "in": "path",
            "name": "vec",
            "required": true,
            "example": ["value1", "value2"],
            "schema": {
              "items": {
                "type": "string"
              },
              "type": "array"
            }
          }
        ]}
    }
}

#[test]
fn path_parameters_with_free_form_properties() {
    let operation = api_fn_doc_with_params! {get: "/foo" =>
        struct MyParams {
            #[param(additional_properties)]
            map: HashMap<String, String>,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq! {
        parameters,
        json!{[
            {
            "in": "path",
            "name": "map",
            "required": true,
            "schema": {
              "additionalProperties": true,
              "type": "object"
            }
          }
        ]}
    }
}

#[test]
fn derive_path_query_params_with_schema_features() {
    let operation = api_fn_doc_with_params! {get: "/foo" =>
        #[into_params(parameter_in = Query)]
        struct MyParams {
            #[serde(default)]
            #[param(write_only, read_only, default = "value", nullable, xml(name = "xml_value"))]
            value: String,
            #[param(value_type = String, format = Binary)]
            int: i64,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq! {
        parameters,
        json!{[
            {
            "in": "query",
            "name": "value",
            "required": false,
            "schema": {
                "default": "value",
                "type": "string",
                "readOnly": true,
                "writeOnly": true,
                "nullable": true,
                "xml": {
                    "name": "xml_value"
                }
            }
          },
          {
            "in": "query",
            "name": "int",
            "required": true,
            "schema": {
              "type": "string",
              "format": "binary"
            }
          }
        ]}
    }
}

#[test]
fn derive_path_params_always_required() {
    let operation = api_fn_doc_with_params! {get: "/foo" =>
        #[into_params(parameter_in = Path)]
        struct MyParams {
            #[serde(default)]
            value: String,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq! {
        parameters,
        json!{[
            {
            "in": "path",
            "name": "value",
            "required": true,
            "schema": {
                "type": "string",
            }
          }
        ]}
    }
}

#[test]
fn derive_required_path_params() {
    let operation = api_fn_doc_with_params! {get: "/list/{id}" =>
        #[into_params(parameter_in = Query)]
        struct MyParams {
            #[serde(default)]
            vec_default: Option<Vec<String>>,

            #[serde(default)]
            string_default: Option<String>,

            #[serde(default)]
            vec_default_required: Vec<String>,

            #[serde(default)]
            string_default_required: String,

            vec_option: Option<Vec<String>>,

            string_option: Option<String>,

            vec: Vec<String>,

            string: String,
        }
    };

    let parameters = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "in": "query",
                "name": "vec_default",
                "required": false,
                "schema": {
                    "type": "array",
                    "nullable": true,
                    "items": {
                        "type": "string"
                    }
                },
            },
            {
                "in": "query",
                "name": "string_default",
                "required": false,
                "schema": {
                    "nullable": true,
                    "type": "string"
                }
            },
            {
                "in": "query",
                "name": "vec_default_required",
                "required": false,
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                },
            },
            {
                "in": "query",
                "name": "string_default_required",
                "required": false,
                "schema": {
                    "type": "string"
                },
            },
            {
                "in": "query",
                "name": "vec_option",
                "required": false,
                "schema": {
                    "nullable": true,
                    "items": {
                        "type": "string"
                    },
                    "type": "array",
                },
            },
            {
                "in": "query",
                "name": "string_option",
                "required": false,
                "schema": {
                    "nullable": true,
                    "type": "string"
                }
            },
            {
                "in": "query",
                "name": "vec",
                "required": true,
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                }
            },
            {
                "in": "query",
                "name": "string",
                "required": true,
                "schema": {
                    "type": "string"
                }
            }
        ])
    )
}

#[test]
fn derive_path_params_with_serde_and_custom_rename() {
    let operation = api_fn_doc_with_params! {get: "/list/{id}" =>
        #[into_params(parameter_in = Query)]
        #[serde(rename_all = "camelCase")]
        struct MyParams {
            vec_default: Option<Vec<String>>,

            #[serde(default, rename = "STRING")]
            string_default: Option<String>,

            #[serde(default, rename = "VEC")]
            #[param(rename = "vec2")]
            vec_default_required: Vec<String>,

            #[serde(default)]
            #[param(rename = "string_r2")]
            string_default_required: String,

            string: String,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "in": "query",
                "name": "vecDefault",
                "required": false,
                "schema": {
                    "type": "array",
                    "nullable": true,
                    "items": {
                        "type": "string"
                    }
                },
            },
            {
                "in": "query",
                "name": "STRING",
                "required": false,
                "schema": {
                    "nullable": true,
                    "type": "string"
                }
            },
            {
                "in": "query",
                "name": "VEC",
                "required": false,
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                },
            },
            {
                "in": "query",
                "name": "string_r2",
                "required": false,
                "schema": {
                    "type": "string"
                },
            },
            {
                "in": "query",
                "name": "string",
                "required": true,
                "schema": {
                    "type": "string"
                }
            }
        ])
    )
}

#[test]
fn derive_path_params_custom_rename_all() {
    let operation = api_fn_doc_with_params! {get: "/list/{id}" =>
        #[into_params(rename_all = "camelCase", parameter_in = Query)]
        struct MyParams {
            vec_default: Option<Vec<String>>,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "in": "query",
                "name": "vecDefault",
                "required": false,
                "schema": {
                    "type": "array",
                    "nullable": true,
                    "items": {
                        "type": "string"
                    }
                },
            },
        ])
    )
}

#[test]
fn derive_path_params_custom_rename_all_serde_will_override() {
    let operation = api_fn_doc_with_params! {get: "/list/{id}" =>
        #[into_params(rename_all = "camelCase", parameter_in = Query)]
        #[serde(rename_all = "UPPERCASE")]
        struct MyParams {
            vec_default: Option<Vec<String>>,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "in": "query",
                "name": "VEC_DEFAULT",
                "required": false,
                "schema": {
                    "type": "array",
                    "nullable": true,
                    "items": {
                        "type": "string"
                    }
                },
            },
        ])
    )
}

#[test]
fn derive_path_parameters_container_level_default() {
    let operation = api_fn_doc_with_params! {get: "/list/{id}" =>
        #[derive(Default)]
        #[into_params(parameter_in = Query)]
        #[serde(default)]
        struct MyParams {
            vec_default: Vec<String>,
            string: String,
        }
    };
    let parameters = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "in": "query",
                "name": "vec_default",
                "required": false,
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                },
            },
            {
                "in": "query",
                "name": "string",
                "required": false,
                "schema": {
                    "type": "string"
                },
            }
        ])
    )
}

#[test]
fn derive_path_params_intoparams() {
    #[derive(serde::Deserialize, ToSchema)]
    #[schema(default = "foo1", example = "foo1")]
    #[serde(rename_all = "snake_case")]
    enum Foo {
        Foo1,
        Foo2,
    }

    #[derive(serde::Deserialize, ToParameters)]
    #[into_params(style = Form, parameter_in = Query)]
    struct MyParams {
        /// Foo database id.
        #[param(example = 1)]
        #[allow(unused)]
        id: i64,
        /// Datetime since foo is updated.
        #[param(example = "2020-04-12T10:23:00Z")]
        #[allow(unused)]
        since: Option<String>,
        /// A Foo item ref.
        #[allow(unused)]
        foo_ref: Foo,
        /// A Foo item inline.
        #[param(inline)]
        #[allow(unused)]
        foo_inline: Foo,
        /// An optional Foo item inline.
        #[param(inline)]
        #[allow(unused)]
        foo_inline_option: Option<Foo>,
        /// A vector of Foo item inline.
        #[param(inline)]
        #[allow(unused)]
        foo_inline_vec: Vec<Foo>,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/list/{id}",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            MyParams,
            ("id" = i64, Path, description = "Id of some items to list")
        )
    )]
    #[allow(unused)]
    fn list(id: i64, params: MyParams) -> String {
        "".to_string()
    }

    let operation: Value = test_api_fn_doc! {
        list,
        operation: get,
        path: "/list/{id}"
    };

    let parameters = operation.get("parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([
            {
                "description": "Foo database id.",
                "example": 1,
                "in": "query",
                "name": "id",
                "required": true,
                "schema": {
                    "format": "int64",
                    "type": "integer",
                },
                "style": "form"
            },
            {
                "description": "Datetime since foo is updated.",
                "example": "2020-04-12T10:23:00Z",
                "in": "query",
                "name": "since",
                "required": false,
                "schema": {
                    "nullable": true,
                    "type": "string"
                },
                "style": "form"
            },
            {
                "description": "A Foo item ref.",
                "in": "query",
                "name": "foo_ref",
                "required": true,
                "schema": {
                    "$ref": "#/components/schemas/Foo"
                },
                "style": "form"
            },
            {
                "description": "A Foo item inline.",
                "in": "query",
                "name": "foo_inline",
                "required": true,
                "schema": {
                    "default": "foo1",
                    "example": "foo1",
                    "enum": ["foo1", "foo2"],
                    "type": "string",
                },
                "style": "form"
            },
            {
                "description": "An optional Foo item inline.",
                "in": "query",
                "name": "foo_inline_option",
                "required": false,
                "schema": {
                    "allOf": [
                        {
                            "default": "foo1",
                            "example": "foo1",
                            "enum": ["foo1", "foo2"],
                            "type": "string",
                        }
                    ],
                    "nullable": true,
                },
                "style": "form"
            },
            {
                "description": "A vector of Foo item inline.",
                "in": "query",
                "name": "foo_inline_vec",
                "required": true,
                "schema": {
                    "items": {
                        "default": "foo1",
                        "example": "foo1",
                        "enum": ["foo1", "foo2"],
                        "type": "string",
                    },
                    "type": "array",
                },
                "style": "form",
            },
            {
                "description": "Id of some items to list",
                "in": "path",
                "name": "id",
                "required": true,
                "schema": {
                    "format": "int64",
                    "type": "integer"
                }
            }
        ])
    )
}

#[test]
fn derive_path_params_into_params_with_value_type() {
    use salvo_oapi::OpenApi;

    #[derive(ToSchema)]
    struct Foo {
        #[allow(unused)]
        value: String,
    }

    #[derive(ToParameters)]
    #[into_params(parameter_in = Query)]
    #[allow(unused)]
    struct Filter {
        #[param(value_type = i64, style = Simple)]
        id: String,
        #[param(value_type = Object)]
        another_id: String,
        #[param(value_type = Vec<Vec<String>>)]
        value1: Vec<i64>,
        #[param(value_type = Vec<String>)]
        value2: Vec<i64>,
        #[param(value_type = Option<String>)]
        value3: i64,
        #[param(value_type = Option<Object>)]
        value4: i64,
        #[param(value_type = Vec<Object>)]
        value5: i64,
        #[param(value_type = Vec<Foo>)]
        value6: i64,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "foo",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            Filter
        )
    )]
    #[allow(unused)]
    fn get_foo(query: Filter) {}

    #[derive(OpenApi, Default)]
    #[openapi(paths(get_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let parameters = doc.pointer("/paths/foo/get/parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([{
            "in": "query",
            "name": "id",
            "required": true,
            "style": "simple",
            "schema": {
                "format": "int64",
                "type": "integer"
            }
        },
        {
            "in": "query",
            "name": "another_id",
            "required": true,
            "schema": {
                "type": "object"
            }
        },
        {
            "in": "query",
            "name": "value1",
            "required": true,
            "schema": {
                "items": {
                    "items": {
                        "type": "string"
                    },
                    "type": "array"
                },
                "type": "array"
            }
        },
        {
            "in": "query",
            "name": "value2",
            "required": true,
            "schema": {
                "items": {
                    "type": "string"
                },
                "type": "array"
            }
        },
        {
            "in": "query",
            "name": "value3",
            "required": false,
            "schema": {
                "nullable": true,
                "type": "string"
            }
        },
        {
            "in": "query",
            "name": "value4",
            "required": false,
            "schema": {
                "nullable": true,
                "type": "object"
            }
        },
        {
            "in": "query",
            "name": "value5",
            "required": true,
            "schema": {
                "items": {
                    "type": "object"
                },
                "type": "array"
            }
        },
        {
            "in": "query",
            "name": "value6",
            "required": true,
            "schema": {
                "items": {
                    "$ref": "#/components/schemas/Foo"
                },
                "type": "array"
            }
        }])
    )
}

#[test]
fn derive_path_params_into_params_with_raw_identifier() {
    #[derive(ToParameters)]
    #[into_params(parameter_in = Path)]
    struct Filter {
        #[allow(unused)]
        r#in: String,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "foo",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            Filter
        )
    )]
    #[allow(unused)]
    fn get_foo(query: Filter) {}

    #[derive(OpenApi, Default)]
    #[openapi(paths(get_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let parameters = doc.pointer("/paths/foo/get/parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([{
            "in": "path",
            "name": "in",
            "required": true,
            "schema": {
                "type": "string"
            }
        }])
    )
}

#[test]
fn derive_path_params_into_params_with_unit_type() {
    #[derive(ToParameters)]
    #[into_params(parameter_in = Path)]
    struct Filter {
        #[allow(unused)]
        r#in: (),
    }

    #[salvo_oapi::endpoint(
        get,
        path = "foo",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            Filter
        )
    )]
    #[allow(unused)]
    fn get_foo(query: Filter) {}

    #[derive(OpenApi, Default)]
    #[openapi(paths(get_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let parameters = doc.pointer("/paths/foo/get/parameters").unwrap();

    assert_json_eq!(
        parameters,
        json!([{
            "in": "path",
            "name": "in",
            "required": true,
            "schema": {
                "default": null,
                "nullable": true
            }
        }])
    )
}

#[test]
fn arbitrary_expr_in_operation_id() {
    #[salvo_oapi::endpoint(
        get,
        path = "foo",
        operation_id=format!("{}", 3+5),
        responses(
            (status = 200, description = "success response")
        ),
    )]
    #[allow(unused)]
    fn get_foo() {}

    #[derive(OpenApi, Default)]
    #[openapi(paths(get_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let operation_id = doc.pointer("/paths/foo/get/operationId").unwrap();

    assert_json_eq!(operation_id, json!("8"))
}

#[test]
fn derive_path_with_validation_attributes() {
    #[derive(ToParameters)]
    #[allow(dead_code)]
    struct Query {
        #[param(maximum = 10, minimum = 5, multiple_of = 2.5)]
        id: i32,

        #[param(max_length = 10, min_length = 5, pattern = "[a-z]*")]
        value: String,

        #[param(max_items = 5, min_items = 1)]
        items: Vec<String>,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "foo",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            Query
        )
    )]
    #[allow(unused)]
    fn get_foo(query: Query) {}

    #[derive(OpenApi, Default)]
    #[openapi(paths(get_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let parameters = doc.pointer("/paths/foo/get/parameters").unwrap();

    let config = Config::new(CompareMode::Strict).numeric_mode(NumericMode::AssumeFloat);

    assert_json_matches!(
        parameters,
        json!([
            {
                "schema": {
                    "format": "int32",
                    "type": "integer",
                    "maximum": 10.0,
                    "minimum": 5.0,
                    "multipleOf": 2.5,
                },
                "required": true,
                "name": "id",
                "in": "path"
            },
            {
                "schema": {
                    "type": "string",
                    "maxLength": 10,
                    "minLength": 5,
                    "pattern": "[a-z]*"
                },
                "required": true,
                "name": "value",
                "in": "path"
            },
            {
                "schema": {
                    "type": "array",
                    "items": {
                        "type": "string",
                    },
                    "maxItems": 5,
                    "minItems": 1,
                },
                "required": true,
                "name": "items",
                "in": "path"
            }
        ]),
        config
    );
}

#[test]
fn derive_path_with_into_responses() {
    #[allow(unused)]
    enum MyResponse {
        Ok,
        NotFound,
    }

    impl IntoResponses for MyResponse {
        fn responses() -> BTreeMap<String, RefOr<Response>> {
            let responses = ResponsesBuilder::new()
                .response("200", ResponseBuilder::new().description("Ok"))
                .response("404", ResponseBuilder::new().description("Not Found"))
                .build();

            responses.responses
        }
    }

    #[salvo_oapi::endpoint(get, path = "foo", responses(MyResponse))]
    #[allow(unused)]
    fn get_foo() {}

    #[derive(OpenApi, Default)]
    #[openapi(paths(get_foo))]
    struct ApiDoc;

    let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let parameters = doc.pointer("/paths/foo/get/responses").unwrap();

    assert_json_eq!(
        parameters,
        json!({
            "200": {
                "description": "Ok"
            },
            "404": {
                "description": "Not Found"
            }
        })
    )
}

#[cfg(feature = "uuid")]
#[test]
fn derive_path_with_uuid() {
    use uuid::Uuid;

    #[salvo_oapi::endpoint(
        get,
        path = "/items/{id}",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            ("id" = Uuid, description = "Foo uuid"),
        )
    )]
    #[allow(unused)]
    fn get_items(id: Uuid) -> String {
        "".to_string()
    }
    let operation = test_api_fn_doc! {
        get_items,
        operation: get,
        path: "/items/{id}"
    };

    assert_value! {operation=>
        "parameters.[0].schema.type" = r#""string""#, "Parameter id type"
        "parameters.[0].schema.format" = r#""uuid""#, "Parameter id format"
        "parameters.[0].description" = r#""Foo uuid""#, "Parameter id description"
        "parameters.[0].name" = r#""id""#, "Parameter id id"
        "parameters.[0].in" = r#""path""#, "Parameter in"
    }
}

#[cfg(feature = "ulid")]
#[test]
fn derive_path_with_ulid() {
    use ulid::Ulid;

    #[salvo_oapi::endpoint(
        get,
        path = "/items/{id}",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            ("id" = Ulid, description = "Foo ulid"),
        )
    )]
    #[allow(unused)]
    fn get_items(id: Ulid) -> String {
        "".to_string()
    }
    let operation = test_api_fn_doc! {
        get_items,
        operation: get,
        path: "/items/{id}"
    };

    assert_value! {operation=>
        "parameters.[0].schema.type" = r#""string""#, "Parameter id type"
        "parameters.[0].schema.format" = r#""ulid""#, "Parameter id format"
        "parameters.[0].description" = r#""Foo ulid""#, "Parameter id description"
        "parameters.[0].name" = r#""id""#, "Parameter id id"
        "parameters.[0].in" = r#""path""#, "Parameter in"
    }
}

#[test]
fn derive_path_with_into_params_custom_schema() {
    fn custom_type() -> Object {
        ObjectBuilder::new()
            .schema_type(salvo_oapi::openapi::SchemaType::String)
            .format(Some(salvo_oapi::openapi::SchemaFormat::Custom(
                "email".to_string(),
            )))
            .description(Some("this is the description"))
            .build()
    }

    #[derive(ToParameters)]
    #[into_params(parameter_in = Query)]
    #[allow(unused)]
    struct Query {
        #[param(schema_with = custom_type)]
        email: String,
    }

    #[salvo_oapi::endpoint(
        get,
        path = "/items",
        responses(
            (status = 200, description = "success response")
        ),
        parameters(
            Query
        )
    )]
    #[allow(unused)]
    fn get_items(query: Query) -> String {
        "".to_string()
    }
    let operation = test_api_fn_doc! {
        get_items,
        operation: get,
        path: "/items"
    };

    let value = operation.pointer("/parameters");

    assert_json_eq!(
        value,
        json!([
            {
                "in": "query",
                "name": "email",
                "required": false,
                "schema": {
                    "description": "this is the description",
                    "type": "string",
                    "format": "email"
                }
            }
        ])
    )
}

#[test]
fn derive_into_params_required() {
    #[derive(ToParameters)]
    #[into_params(parameter_in = Query)]
    #[allow(unused)]
    struct Params {
        name: String,
        name2: Option<String>,
        #[param(required)]
        name3: Option<String>,
    }

    #[salvo_oapi::endpoint(get, params(Params))]
    #[allow(unused)]
    fn get_params() {}
    let operation = test_api_fn_doc! {
        get_params,
        operation: get,
        path: "/params"
    };

    let value = operation.pointer("/parameters");

    assert_json_eq!(
        value,
        json!([
          {
              "in": "query",
              "name": "name",
              "required": true,
              "schema": {
                  "type": "string",
              },
          },
          {
              "in": "query",
              "name": "name2",
              "required": false,
              "schema": {
                  "type": "string",
                  "nullable": true,
              },
          },
          {
              "in": "query",
              "name": "name3",
              "required": true,
              "schema": {
                  "type": "string",
                  "nullable": true,
              },
          },
        ])
    )
}

#[test]
fn derive_into_params_with_serde_skip() {
    #[derive(ToParameters, Serialize)]
    #[into_params(parameter_in = Query)]
    #[allow(unused)]
    struct Params {
        name: String,
        name2: Option<String>,
        #[serde(skip)]
        name3: Option<String>,
    }

    #[salvo_oapi::endpoint(get, path = "/params", params(Params))]
    #[allow(unused)]
    fn get_params() {}
    let operation = test_api_fn_doc! {
        get_params,
        operation: get,
        path: "/params"
    };

    let value = operation.pointer("/parameters");

    assert_json_eq!(
        value,
        json!([
          {
              "in": "query",
              "name": "name",
              "required": true,
              "schema": {
                  "type": "string",
              },
          },
          {
              "in": "query",
              "name": "name2",
              "required": false,
              "schema": {
                  "type": "string",
                  "nullable": true,
              },
          },
        ])
    )
}

#[test]
fn derive_into_params_with_serde_skip_deserializing() {
    #[derive(ToParameters, Serialize)]
    #[into_params(parameter_in = Query)]
    #[allow(unused)]
    struct Params {
        name: String,
        name2: Option<String>,
        #[serde(skip_deserializing)]
        name3: Option<String>,
    }

    #[salvo_oapi::endpoint(get, path = "/params", params(Params))]
    #[allow(unused)]
    fn get_params() {}
    let operation = test_api_fn_doc! {
        get_params,
        operation: get,
        path: "/params"
    };

    let value = operation.pointer("/parameters");

    assert_json_eq!(
        value,
        json!([
          {
              "in": "query",
              "name": "name",
              "required": true,
              "schema": {
                  "type": "string",
              },
          },
          {
              "in": "query",
              "name": "name2",
              "required": false,
              "schema": {
                  "type": "string",
                  "nullable": true,
              },
          },
        ])
    )
}

#[test]
fn derive_into_params_with_serde_skip_serializing() {
    #[derive(ToParameters, Serialize)]
    #[into_params(parameter_in = Query)]
    #[allow(unused)]
    struct Params {
        name: String,
        name2: Option<String>,
        #[serde(skip_serializing)]
        name3: Option<String>,
    }

    #[salvo_oapi::endpoint(get, path = "/params", params(Params))]
    #[allow(unused)]
    fn get_params(params: QueryParam<Params>) {}
    let router = Router::with_path("params", get_params);

    let operation = test_api_fn_doc! {
        get_params,
        operation: get,
        path: "/params"
    };


    let value = operation.pointer("/parameters");

    assert_json_eq!(
        value,
        json!([
          {
              "in": "query",
              "name": "name",
              "required": true,
              "schema": {
                  "type": "string",
              },
          },
          {
              "in": "query",
              "name": "name2",
              "required": false,
              "schema": {
                  "type": "string",
                  "nullable": true,
              },
          },
        ])
    )
}
