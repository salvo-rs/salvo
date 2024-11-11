#![allow(missing_docs)]
use assert_json_diff::assert_json_eq;
use salvo::oapi::extract::*;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[test]
fn test_derive_to_schema_generics() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(aliases(MyI32 = MyObject<i32>, MyStr = MyObject<String>)))]
    struct MyObject<T: ToSchema + std::fmt::Debug + 'static> {
        value: T,
    }

    /// Use string type, this will add to openapi doc.
    #[endpoint]
    async fn use_string(body: JsonBody<MyObject<String>>) -> String {
        format!("{:?}", body)
    }

    /// Use i32 type, this will add to openapi doc.
    #[endpoint]
    async fn use_i32(body: JsonBody<MyObject<i32>>) -> String {
        format!("{:?}", body)
    }

    /// Use u64 type, this will add to openapi doc.
    #[endpoint]
    async fn use_u64(body: JsonBody<MyObject<u64>>) -> String {
        format!("{:?}", body)
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new()
        .push(Router::with_path("i32").post(use_i32))
        .push(Router::with_path("u64").post(use_u64))
        .push(Router::with_path("string").post(use_string));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();
    assert_json_eq!(
        schemas,
        json!({
            "MyObject_String_": {
                "type": "object",
                "required": [
                    "value"
                ],
                "properties": {
                    "value": {
                        "type": "string"
                    }
                }
            },
            "MyObject_i32_": {
                "type": "object",
                "required": [
                    "value"
                ],
                "properties": {
                    "value": {
                        "type": "integer",
                        "format": "int32"
                    }
                }
            },
            "MyObject_u64_": {
                "type": "object",
                "required": [
                    "value"
                ],
                "properties": {
                    "value": {
                        "type": "integer",
                        "format": "uint64",
                        "minimum": 0.0
                    }
                }
            }
        })
    );
    let paths = value.pointer("/paths").unwrap();
    assert_json_eq!(
        paths,
        json!({
            "/i32": {
                "post": {
                    "summary": "Use i32 type, this will add to openapi doc.",
                    "operationId": "use_i32",
                    "requestBody": {
                        "description": "Extract json format data from request.",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/MyObject_i32_"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Ok",
                            "content": {
                                "text/plain": {
                                    "schema": {
                                        "type": "string"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/string": {
                "post": {
                    "summary": "Use string type, this will add to openapi doc.",
                    "operationId": "use_string",
                    "requestBody": {
                        "description": "Extract json format data from request.",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/MyObject_String_"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Ok",
                            "content": {
                                "text/plain": {
                                    "schema": {
                                        "type": "string"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/u64": {
                "post": {
                    "summary": "Use u64 type, this will add to openapi doc.",
                    "operationId": "use_u64",
                    "requestBody": {
                        "description": "Extract json format data from request.",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/MyObject_u64_"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Ok",
                            "content": {
                                "text/plain": {
                                    "schema": {
                                        "type": "string"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    );
}

#[test]
fn test_derive_to_schema_enum() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(rename_all = "camelCase"))]
    enum People {
        Man,
        Woman,
    }

    #[endpoint]
    async fn hello(body: JsonBody<People>) -> String {
        format!("{:?}", body)
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("hello").post(hello));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);
    println!("{}", doc.to_json().unwrap());
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();
    assert_json_eq!(
        schemas,
        json!({
            "People": {
                "type": "string",
                "enum": [
                    "man",
                    "woman"
                ]
            }
        })
    );
    let paths = value.pointer("/paths").unwrap();
    assert_json_eq!(
        paths,
        json!({
            "/hello": {
                "post": {
                    "operationId": "hello",
                    "requestBody": {
                        "description": "Extract json format data from request.",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/People"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Ok",
                            "content": {
                                "text/plain": {
                                    "schema": {
                                        "type": "string"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    );
}
