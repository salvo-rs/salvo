use assert_json_diff::assert_json_eq;
use salvo_oapi::ToSchema;
use salvo_oapi_gen::ToResponse;
use serde_json::json;

#[test]
fn derive_name_struct_response() {
    #[derive(ToResponse)]
    #[allow(unused)]
    struct Person {
        name: String,
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "type": "object",
                        "required": ["name"]
                    }
                }
            },
            "description": ""
        })
    )
}

#[test]
fn derive_unnamed_struct_response() {
    #[derive(ToResponse)]
    #[allow(unused)]
    struct Person(Vec<String>);

    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "items": {
                            "type": "string"
                        },
                        "type": "array"
                    }
                }
            },
            "description": ""
        })
    )
}

#[test]
fn derive_enum_response() {
    #[derive(ToResponse)]
    #[allow(unused)]
    enum PersonType {
        Value(String),
        Foobar,
    }
    let (name, v) = <PersonType as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("PersonType", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "oneOf": [
                        {
                            "properties": {
                                "Value": {
                                    "type": "string"
                                }
                            },
                            "required": ["Value"],
                            "type": "object",
                        },
                        {
                            "enum": ["Foobar"],
                            "type": "string"
                        }
                        ]
                    }
                }
            },
            "description": ""
        })
    )
}

#[test]
fn derive_struct_response_with_description() {
    /// This is description
    ///
    /// It will also be used in `ToSchema` if present
    #[derive(ToResponse)]
    #[allow(unused)]
    struct Person {
        name: String,
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "description": "This is description\n\nIt will also be used in `ToSchema` if present",
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "type": "object",
                        "required": ["name"]
                    }
                }
            },
            "description": "This is description\n\nIt will also be used in `ToSchema` if present"
        })
    )
}

#[test]
fn derive_response_with_attributes() {
    /// This is description
    ///
    /// It will also be used in `ToSchema` if present
    #[derive(ToSchema, ToResponse)]
    #[response(description = "Override description for response", content_type = "text/xml")]
    #[response(
        example = json!({"name": "the name"}),
        headers(
            ("csrf-token", description = "response csrf token"),
            ("random-id" = i32)
        )
    )]
    #[allow(unused)]
    struct Person {
        name: String,
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "text/xml": {
                    "example": {
                        "name": "the name"
                    },
                    "schema": {
                        "description": "This is description\n\nIt will also be used in `ToSchema` if present",
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "type": "object",
                        "required": ["name"]
                    }
                }
            },
            "description": "Override description for response",
            "headers": {
                "csrf-token": {
                    "description": "response csrf token",
                    "schema": {
                        "type": "string"
                    }
                },
                "random-id": {
                    "schema": {
                        "type": "integer",
                        "format": "int32"
                    }
                }
            }
        })
    )
}

#[test]
fn derive_response_with_multiple_content_types() {
    #[derive(ToSchema, ToResponse)]
    #[response(content_type = ["application/json", "text/xml"] )]
    #[allow(unused)]
    struct Person {
        name: String,
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "type": "object",
                        "required": ["name"]
                    }
                },
                "text/xml": {
                    "schema": {
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "type": "object",
                        "required": ["name"]
                    }
                }
            },
            "description": ""
        })
    )
}

#[test]
fn derive_response_multiple_examples() {
    #[derive(ToSchema, ToResponse)]
    #[response(examples(
            ("Person1" = (value = json!({"name": "name1"}))),
            ("Person2" = (value = json!({"name": "name2"})))
    ))]
    #[allow(unused)]
    struct Person {
        name: String,
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "examples": {
                        "Person1": {
                            "value": {
                                "name": "name1"
                            }
                        },
                        "Person2": {
                            "value": {
                                "name": "name2"
                            }
                        }
                    },
                    "schema": {
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "type": "object",
                        "required": ["name"]
                    }
                },
            },
            "description": ""
        })
    )
}

#[test]
fn derive_response_with_enum_contents() {
    #[allow(unused)]
    struct Admin {
        name: String,
    }
    #[allow(unused)]
    struct Moderator {
        name: String,
    }
    #[derive(ToSchema, ToResponse)]
    #[allow(unused)]
    enum Person {
        #[response(examples(
                ("Person1" = (value = json!({"name": "name1"}))),
                ("Person2" = (value = json!({"name": "name2"})))
        ))]
        Admin(#[content("application/json/1")] Admin),
        #[response(example = json!({"name": "name3"}))]
        Moderator(#[content("application/json/2")] Moderator),
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json/1": {
                    "examples": {
                        "Person1": {
                            "value": {
                                "name": "name1"
                            }
                        },
                        "Person2": {
                            "value": {
                                "name": "name2"
                            }
                        }
                    },
                    "schema": {
                        "$ref": "#/components/schemas/Admin"
                    }
                },
                "application/json/2": {
                    "example": {
                        "name": "name3"
                    },
                    "schema": {
                        "$ref": "#/components/schemas/Moderator"
                    }
                }
            },
            "description": ""
        })
    )
}

#[test]
fn derive_response_with_enum_contents_inlined() {
    #[allow(unused)]
    #[derive(ToSchema)]
    struct Admin {
        name: String,
    }

    #[derive(ToSchema)]
    #[allow(unused)]
    struct Moderator {
        name: String,
    }
    #[derive(ToSchema, ToResponse)]
    #[allow(unused)]
    enum Person {
        #[response(examples(
                ("Person1" = (value = json!({"name": "name1"}))),
                ("Person2" = (value = json!({"name": "name2"})))
        ))]
        Admin(
            #[content("application/json/1")]
            #[to_schema]
            Admin,
        ),
        #[response(example = json!({"name": "name3"}))]
        Moderator(
            #[content("application/json/2")]
            #[to_schema]
            Moderator,
        ),
    }
    let (name, v) = <Person as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("Person", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json/1": {
                    "examples": {
                        "Person1": {
                            "value": {
                                "name": "name1"
                            }
                        },
                        "Person2": {
                            "value": {
                                "name": "name2"
                            }
                        }
                    },
                    "schema": {
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "required": ["name"],
                        "type": "object"
                    }
                },
                "application/json/2": {
                    "example": {
                        "name": "name3"
                    },
                    "schema": {
                        "properties": {
                            "name": {
                                "type": "string"
                            }
                        },
                        "required": ["name"],
                        "type": "object"
                    }
                }
            },
            "description": ""
        })
    )
}

#[test]
fn derive_response_with_unit_type() {
    #[derive(ToSchema, ToResponse)]
    #[allow(unused)]
    struct PersonSuccessResponse;

    let (name, v) = <PersonSuccessResponse as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("PersonSuccessResponse", name);
    assert_json_eq!(
        value,
        json!({
            "description": ""
        })
    )
}

#[test]
fn derive_response_with_inline_unnamed_schema() {
    #[allow(unused)]
    #[derive(ToSchema)]
    struct Person {
        name: String,
    }
    #[derive(ToResponse)]
    #[allow(unused)]
    struct PersonSuccessResponse(#[to_schema] Vec<Person>);

    let (name, v) = <PersonSuccessResponse as salvo_oapi::ToResponse>::response();
    let value = serde_json::to_value(v).unwrap();

    assert_eq!("PersonSuccessResponse", name);
    assert_json_eq!(
        value,
        json!({
            "content": {
                "application/json": {
                    "schema": {
                        "items": {
                            "properties": {
                                "name": {
                                    "type": "string"
                                }
                            },
                            "required": ["name"],
                            "type": "object",
                        },
                        "type": "array"
                    }
                },
            },
            "description": ""
        })
    )
}

macro_rules! into_responses {
    ( $(#[$meta:meta])* $key:ident $ident:ident $($tt:tt)* ) => {
        {
            #[derive(salvo_oapi::IntoResponses)]
            $(#[$meta])*
            #[allow(unused)]
            $key $ident $( $tt )*

            let responses = <$ident as salvo_oapi::IntoResponses>::responses();
            serde_json::to_value(responses).unwrap()
        }
    };
}

#[test]
fn derive_into_responses_inline_named_struct_response() {
    let responses = into_responses! {
        /// This is success response
        #[response(status = 200)]
        struct SuccessResponse {
            value: String,
        }
    };

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/json": {
                        "schema": {
                            "description": "This is success response",
                            "properties": {
                                "value": {
                                    "type": "string"
                                },
                            },
                            "required": ["value"],
                            "type": "object"
                        }
                    }
                },
                "description": "This is success response"
            }
        })
    )
}

#[test]
fn derive_into_responses_unit_struct() {
    let responses = into_responses! {
        /// Not found response
        #[response(status = NOT_FOUND)]
        struct NotFound;
    };

    assert_json_eq!(
        responses,
        json!({
            "404": {
                "description": "Not found response"
            }
        })
    )
}

#[test]
fn derive_into_responses_unnamed_struct_inline_schema() {
    #[derive(salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct Foo {
        bar: String,
    }

    let responses = into_responses! {
        #[response(status = 201)]
        struct CreatedResponse(#[to_schema] Foo);
    };

    assert_json_eq!(
        responses,
        json!({
            "201": {
                "content": {
                    "application/json": {
                        "schema": {
                            "properties": {
                                "bar": {
                                    "type": "string"
                                },
                            },
                            "required": ["bar"],
                            "type": "object"
                        }
                    }
                },
                "description": ""
            }
        })
    )
}

#[test]
fn derive_into_responses_unnamed_struct_with_primitive_schema() {
    let responses = into_responses! {
        #[response(status = 201)]
        struct CreatedResponse(String);
    };

    assert_json_eq!(
        responses,
        json!({
            "201": {
                "content": {
                    "text/plain": {
                        "schema": {
                            "type": "string",
                        }
                    }
                },
                "description": ""
            }
        })
    )
}

#[test]
fn derive_into_responses_unnamed_struct_ref_schema() {
    #[derive(salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct Foo {
        bar: String,
    }

    let responses = into_responses! {
        #[response(status = 201)]
        struct CreatedResponse(Foo);
    };

    assert_json_eq!(
        responses,
        json!({
            "201": {
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": "#/components/schemas/Foo",
                        }
                    }
                },
                "description": ""
            }
        })
    )
}

#[test]
fn derive_into_responses_unnamed_struct_ref_response() {
    #[derive(salvo_oapi::ToResponse)]
    #[allow(unused)]
    struct Foo {
        bar: String,
    }

    let responses = into_responses! {
        #[response(status = 201)]
        struct CreatedResponse(#[ref_response] Foo);
    };

    assert_json_eq!(
        responses,
        json!({
            "201": {
                "$ref": "#/components/responses/Foo"
            }
        })
    )
}

#[test]
fn derive_into_responses_unnamed_struct_to_response() {
    #[derive(salvo_oapi::ToResponse)]
    #[allow(unused)]
    struct Foo {
        bar: String,
    }

    let responses = into_responses! {
        #[response(status = 201)]
        struct CreatedResponse(#[to_response] Foo);
    };

    assert_json_eq!(
        responses,
        json!({
            "201": {
                "content": {
                    "application/json": {
                        "schema": {
                            "properties": {
                                "bar": {
                                    "type": "string"
                                }
                            },
                            "required": ["bar"],
                            "type": "object",
                        }
                    }
                },
                "description": ""
            }
        })
    )
}

#[test]
fn derive_into_responses_enum_with_multiple_responses() {
    #[derive(salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct BadRequest {
        value: String,
    }

    #[derive(salvo_oapi::ToResponse)]
    #[allow(unused)]
    struct Response {
        message: String,
    }

    let responses = into_responses! {
        enum UserResponses {
            /// Success response
            #[response(status = 200)]
            Success { value: String },

            #[response(status = 404)]
            NotFound,

            #[response(status = 400)]
            BadRequest(BadRequest),

            #[response(status = 500)]
            ServerError(#[ref_response] Response),

            #[response(status = 418)]
            TeaPot(#[to_response] Response),
        }
    };

    assert_json_eq!(
        responses,
        json!({
            "200": {
                "content": {
                    "application/json": {
                        "schema": {
                            "properties": {
                                "value": {
                                    "type": "string"
                                }
                            },
                            "description": "Success response",
                            "required": ["value"],
                            "type": "object",
                        }
                    }
                },
                "description": "Success response"
            },
            "400": {
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": "#/components/schemas/BadRequest"
                        }
                    }
                },
                "description": "",
            },
            "404": {
                "description": ""
            },
            "418": {
                "content": {
                    "application/json": {
                        "schema": {
                            "properties": {
                                "message": {
                                    "type": "string"
                                }
                            },
                            "required": ["message"],
                            "type": "object",
                        }
                    }
                },
                "description": "",
            },
            "500": {
                "$ref": "#/components/responses/Response"
            }
        })
    )
}
