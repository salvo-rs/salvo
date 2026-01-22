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
        format!("{body:?}")
    }

    /// Use i32 type, this will add to openapi doc.
    #[endpoint]
    async fn use_i32(body: JsonBody<MyObject<i32>>) -> String {
        format!("{body:?}")
    }

    /// Use u64 type, this will add to openapi doc.
    #[endpoint]
    async fn use_u64(body: JsonBody<MyObject<u64>>) -> String {
        format!("{body:?}")
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
            "MyStr_String_": {
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
            "MyI32_i32_": {
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
                                    "$ref": "#/components/schemas/MyI32_i32_"
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
                                    "$ref": "#/components/schemas/MyStr_String_"
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
        format!("{body:?}")
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

#[test]
fn test_derive_to_schema_new_type_struct() {
    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(multiple_of = 5))]
    struct MultipleOfType(i32);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(maximum = 100))]
    struct MaximumType(u32);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(minimum = -100))]
    struct MinimumType(i32);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(exclusive_maximum = 100))]
    struct ExclusiveMaximumType(i64);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(exclusive_minimum = -100))]
    struct ExclusiveMinimumType(i64);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(min_length = 3))]
    struct MinLengthType(String);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(max_length = 3))]
    struct MaxLengthType(String);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(pattern = r#"^([a-zA-Z0-9_\-]{3,32}$)"#))]
    struct PatternType(String);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(max_items = 5))]
    struct MaxItemsType(Vec<String>);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    #[salvo(schema(min_items = 1))]
    struct MinItemsType(Vec<String>);

    #[derive(Serialize, Deserialize, ToSchema, Debug)]
    struct SomeDto {
        pub multiple_of: MultipleOfType,
        pub maximum: MaximumType,
        pub minimum: MinimumType,
        pub exclusive_maximum: ExclusiveMaximumType,
        pub exclusive_minimum: ExclusiveMinimumType,
        pub min_length: MinLengthType,
        pub max_length: MaxLengthType,
        pub pattern: PatternType,
        pub max_items: MaxItemsType,
        pub min_items: MinItemsType,
    }

    #[endpoint]
    async fn new_type(body: JsonBody<SomeDto>) -> String {
        format!("{body:?}")
    }

    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new().push(Router::with_path("new-type").post(new_type));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);
    println!("{}", doc.to_json().unwrap());
    let value = serde_json::to_value(&doc).unwrap();
    let schemas = value.pointer("/components/schemas").unwrap();
    assert_json_eq!(
        schemas,
        json!({
            "ExclusiveMaximumType": {
                "type": "integer",
                "format": "int64",
                "exclusiveMaximum": 100.0
            },
            "ExclusiveMinimumType": {
                "type": "integer",
                "format": "int64",
                "exclusiveMinimum": -100.0
            },
            "MaxItemsType": {
                "type": "array",
                "items": {
                    "type": "string"
                },
                "maxItems": 5
            },
            "MaxLengthType": {
                "type": "string",
                "maxLength": 3
            },
            "MaximumType": {
                "type": "integer",
                "format": "uint32",
                "maximum": 100.0,
                "minimum": 0.0
            },
            "MinItemsType": {
                "type": "array",
                "items": {
                    "type": "string"
                },
                "minItems": 1
            },
            "MinLengthType": {
                "type": "string",
                "minLength": 3
            },
            "MinimumType": {
                "type": "integer",
                "format": "int32",
                "minimum": -100.0
            },
            "MultipleOfType": {
                "type": "integer",
                "format": "int32",
                "multipleOf": 5.0
            },
            "PatternType": {
                "type": "string",
                "pattern": "^([a-zA-Z0-9_\\-]{3,32}$)"
            },
            "SomeDto": {
                "type": "object",
                "required": [
                    "multiple_of",
                    "maximum",
                    "minimum",
                    "exclusive_maximum",
                    "exclusive_minimum",
                    "min_length",
                    "max_length",
                    "pattern",
                    "max_items",
                    "min_items"
                ],
                "properties": {
                    "exclusive_maximum": {
                        "$ref": "#/components/schemas/ExclusiveMaximumType"
                    },
                    "exclusive_minimum": {
                        "$ref": "#/components/schemas/ExclusiveMinimumType"
                    },
                    "max_items": {
                        "$ref": "#/components/schemas/MaxItemsType"
                    },
                    "max_length": {
                        "$ref": "#/components/schemas/MaxLengthType"
                    },
                    "maximum": {
                        "$ref": "#/components/schemas/MaximumType"
                    },
                    "min_items": {
                        "$ref": "#/components/schemas/MinItemsType"
                    },
                    "min_length": {
                        "$ref": "#/components/schemas/MinLengthType"
                    },
                    "minimum": {
                        "$ref": "#/components/schemas/MinimumType"
                    },
                    "multiple_of": {
                        "$ref": "#/components/schemas/MultipleOfType"
                    },
                    "pattern": {
                        "$ref": "#/components/schemas/PatternType"
                    }
                }
            }
        })
    );
    let paths = value.pointer("/paths").unwrap();
    assert_json_eq!(
        paths,
        json!({
            "/new-type": {
                "post": {
                    "operationId": "new_type",
                    "requestBody": {
                        "description": "Extract json format data from request.",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/SomeDto"
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
