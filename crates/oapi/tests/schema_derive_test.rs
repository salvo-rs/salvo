use std::{borrow::Cow, cell::RefCell, collections::HashMap, marker::PhantomData, vec};

use assert_json_diff::{assert_json_eq, assert_json_matches, CompareMode, Config, NumericMode};
use salvo_oapi::openapi::{Object, ObjectBuilder};
use salvo_oapi::{OpenApi, ToSchema, TupleUnit};
use serde::Serialize;
use serde_json::{json, Value};

mod common;

macro_rules! api_doc {
    ( $(#[$meta:meta])* $key:ident $ident:ident $($tt:tt)* ) => {
        {
            #[derive(ToSchema)]
            $(#[$meta])*
            #[allow(unused)]
            $key $ident $( $tt )*

            let schema = api_doc!( @schema $ident $($tt)* );
            serde_json::to_value(schema).unwrap()
        }
    };
    ( @schema $ident:ident < $($life:lifetime , )? $generic:ident > $($tt:tt)* ) => {
         <$ident<$generic> as salvo_oapi::ToSchema>::schema().1
    };
    ( @schema $ident:ident $($tt:tt)* ) => {
         <$ident as salvo_oapi::ToSchema>::schema().1
    };
}

macro_rules! api_doc_aliases {
    ( $(#[$meta:meta])* $key:ident $ident:ident $($tt:tt)* ) => {
        {
            #[derive(ToSchema)]
            $(#[$meta])*
            #[allow(unused)]
            $key $ident $( $tt )*

            let schema = api_doc_aliases!( @schema $ident $($tt)* );
            serde_json::to_value(schema).unwrap()
        }
    };
    ( @schema $ident:ident < $($life:lifetime , )? $generic:ident > $($tt:tt)* ) => {
         <$ident<$generic> as salvo_oapi::ToSchema>::aliases()
    };
    ( @schema $ident:ident $($tt:tt)* ) => {
         <$ident as salvo_oapi::ToSchema>::aliases()
    };
}

#[test]
fn derive_map_type() {
    let map = api_doc! {
        struct Map {
            map: HashMap<String, String>,
        }
    };

    assert_value! { map=>
        "properties.map.additionalProperties.type" = r#""string""#, "Additional Property Type"
    };
}

#[test]
fn derive_map_ref() {
    #[derive(ToSchema)]
    #[allow(unused)]
    enum Foo {
        Variant,
    }

    let map = api_doc! {
        struct Map {
            map: HashMap<String, Foo>,
            #[schema(inline)]
            map2: HashMap<String, Foo>
        }
    };

    assert_json_eq!(
        map,
        json!({
            "properties": {
                "map": {
                    "additionalProperties": {
                        "$ref": "#/components/schemas/Foo"
                    },
                    "type": "object",
                },
                "map2": {
                    "additionalProperties": {
                        "type": "string",
                        "enum": ["Variant"]
                    },
                    "type": "object"
                }
            },
            "required": ["map", "map2"],
            "type": "object"
        })
    )
}

#[test]
fn derive_map_free_form_property() {
    let map = api_doc! {
        struct Map {
            #[schema(additional_properties)]
            map: HashMap<String, String>,
        }
    };

    assert_json_eq!(
        map,
        json!({
            "properties": {
                "map": {
                    "additionalProperties": true,
                    "type": "object",
                },
            },
            "required": ["map"],
            "type": "object"
        })
    )
}

#[test]
fn derive_enum_with_additional_properties_success() {
    let mode = api_doc! {
        #[schema(default = "Mode1", example = "Mode2")]
        enum Mode {
            Mode1, Mode2
        }
    };

    assert_value! {mode=>
        "default" = r#""Mode1""#, "Mode default"
        "example" = r#""Mode2""#, "Mode example"
        "enum" = r#"["Mode1","Mode2"]"#, "Mode enum variants"
        "type" = r#""string""#, "Mode type"
    };
}

#[test]
fn derive_enum_with_defaults_success() {
    let mode = api_doc! {
        enum Mode {
            Mode1,
            Mode2
        }
    };

    assert_value! {mode=>
        "enum" = r#"["Mode1","Mode2"]"#, "Mode enum variants"
        "type" = r#""string""#, "Mode type"
    };
    assert_value! {mode=>
        "default" = Value::Null, "Mode default"
        "example" = Value::Null, "Mode example"
    }
}

#[test]
fn derive_enum_with_with_custom_default_fn_success() {
    let mode = api_doc! {
        #[schema(default = mode_custom_default_fn)]
        enum Mode {
            Mode1,
            Mode2
        }
    };

    assert_value! {mode=>
        "default" = r#""Mode2""#, "Mode default"
        "enum" = r#"["Mode1","Mode2"]"#, "Mode enum variants"
        "type" = r#""string""#, "Mode type"
    };
    assert_value! {mode=>
        "example" = Value::Null, "Mode example"
    }
}

fn mode_custom_default_fn() -> String {
    "Mode2".to_string()
}

#[test]
fn derive_struct_with_defaults_success() {
    let book = api_doc! {
        struct Book {
            name: String,
            hash: String,
        }
    };

    assert_value! {book=>
        "type" = r#""object""#, "Book type"
        "properties.name.type" = r#""string""#, "Book name type"
        "properties.hash.type" = r#""string""#, "Book hash type"
        "required" = r#"["name","hash"]"#, "Book required fields"
    };
}

#[test]
fn derive_struct_with_custom_properties_success() {
    let book = api_doc! {
        struct Book {
            #[schema(default = String::default)]
            name: String,
            #[schema(
                default = "testhash",
                example = "base64 text",
                format = Byte,
            )]
            hash: String,
        }
    };

    assert_value! {book=>
        "type" = r#""object""#, "Book type"
        "properties.name.type" = r#""string""#, "Book name type"
        "properties.name.default" = r#""""#, "Book name default"
        "properties.hash.type" = r#""string""#, "Book hash type"
        "properties.hash.format" = r#""byte""#, "Book hash format"
        "properties.hash.example" = r#""base64 text""#, "Book hash example"
        "properties.hash.default" = r#""testhash""#, "Book hash default"
        "required" = r#"["name","hash"]"#, "Book required fields"
    };
}

#[test]
fn derive_struct_with_default_attr() {
    let book = api_doc! {
        #[schema(default)]
        struct Book {
            name: String,
            #[schema(default = 0)]
            id: u64,
            year: u64,
            hash: String,
        }

        impl Default for Book {
            fn default() -> Self {
                Self {
                    name: "No name".to_string(),
                    id: 999,
                    year: 2020,
                    hash: "Test hash".to_string(),
                }
            }
        }
    };

    assert_value! { book =>
        "properties.name.default" = r#""No name""#, "Book name default"
        "properties.id.default" = r#"0"#, "Book id default"
        "properties.year.default" = r#"2020"#, "Book year default"
        "properties.hash.default" = r#""Test hash""#, "Book hash default"
    };
}

#[test]
fn derive_struct_with_default_attr_field() {
    struct Book;
    let owner = api_doc! {
        struct Owner {
            #[schema(default = json!({ "name": "Dune" }))]
            favorite_book: Book,
            #[schema(default = json!([{ "name": "The Fellowship Of The Ring" }]))]
            books: Vec<Book>,
            #[schema(default = json!({ "National Library": { "name": "The Stranger" } }))]
            leases: HashMap<String, Book>,
            #[schema(default = json!({ "name": "My Book" }))]
            authored: Option<Book>,
        }
    };

    assert_json_eq!(
        owner,
        json!({
            "properties": {
                "favorite_book": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/Book",
                        },
                    ],
                    "default": {
                        "name": "Dune",
                    },
                },
                "books": {
                    "items": {
                        "$ref": "#/components/schemas/Book",
                    },
                    "type": "array",
                    "default": [
                        {
                            "name": "The Fellowship Of The Ring"
                        }
                    ]
                },
                "leases": {
                    "additionalProperties": {
                        "$ref": "#/components/schemas/Book",
                    },
                    "default": {
                        "National Library": {
                            "name": "The Stranger",
                        },
                    },
                    "type": "object",
                },
                "authored": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/Book",
                        },
                    ],
                    "default": {
                        "name": "My Book",
                    },
                    "nullable": true,
                },
            },
            "required": [
                "favorite_book",
                "books",
                "leases",
            ],
            "type": "object",
        })
    );
}

#[test]
fn derive_struct_with_serde_default_attr() {
    let book = api_doc! {
        #[derive(serde::Deserialize)]
        #[serde(default)]
        struct Book {
            name: String,
            #[schema(default = 0)]
            id: u64,
            year: u64,
            hash: String,
        }

        impl Default for Book {
            fn default() -> Self {
                Self {
                    name: "No name".to_string(),
                    id: 999,
                    year: 2020,
                    hash: "Test hash".to_string(),
                }
            }
        }
    };

    assert_value! { book =>
        "properties.name.default" = r#""No name""#, "Book name default"
        "properties.id.default" = r#"0"#, "Book id default"
        "properties.year.default" = r#"2020"#, "Book year default"
        "properties.hash.default" = r#""Test hash""#, "Book hash default"
    };
}

#[test]
fn derive_struct_with_optional_properties() {
    struct Book;
    let owner = api_doc! {
        struct Owner {
            #[schema(default = 1)]
            id: i64,
            enabled: Option<bool>,
            books: Option<Vec<Book>>,
            metadata: Option<HashMap<String, String>>,
            optional_book: Option<Book>
        }
    };

    assert_json_eq!(
        owner,
        json!({
            "properties": {
                "id": {
                    "type": "integer",
                    "format": "int64",
                    "default": 1,
                },
                "enabled": {
                    "type": "boolean",
                    "nullable": true,
                },
                "books": {
                    "items": {
                        "$ref": "#/components/schemas/Book",
                    },
                    "nullable": true,
                    "type": "array"
                },
                "metadata": {
                    "type": "object",
                    "nullable": true,
                    "additionalProperties": {
                        "type": "string"
                    }
                },
                "optional_book": {
                    "nullable": true,
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/Book"
                        }
                    ]
                }
            },
            "required": [
                "id",
            ],
            "type": "object"
        })
    );
}

#[test]
fn derive_struct_with_comments() {
    struct Foobar;
    let account = api_doc! {
        /// This is user account dto object
        ///
        /// Detailed documentation here.
        /// More than the first line is added to the description as well.
        struct Account {
            /// Database autogenerated id
            id: i64,
            /// Users username
            username: String,
            /// Role ids
            role_ids: Vec<i32>,
            /// Foobars
            foobars: Vec<Foobar>,
            /// Map description
            map: HashMap<String, String>
        }
    };

    assert_json_eq!(
        account,
        json!({
            "description": "This is user account dto object\n\nDetailed documentation here.\nMore than the first line is added to the description as well.",
            "properties": {
                "foobars": {
                    "description": "Foobars",
                    "type": "array",
                    "items": {
                        "$ref": "#/components/schemas/Foobar"
                    }
                },
                "id": {
                    "type": "integer",
                    "format": "int64",
                    "description": "Database autogenerated id",
                },
                "role_ids": {
                    "description": "Role ids",
                    "type": "array",
                    "items": {
                        "type": "integer",
                        "format": "int32",
                    }
                },
                "username": {
                    "type": "string",
                    "description": "Users username",
                },
                "map": {
                    "description": "Map description",
                    "type": "object",
                    "additionalProperties": {
                        "type": "string"
                    },
                }
            },
            "required": ["id", "username", "role_ids", "foobars", "map"],
            "type": "object"
        })
    )
}

#[test]
fn derive_enum_with_comments_success() {
    let account = api_doc! {
        /// This is user account status enum
        ///
        /// Detailed documentation here.
        /// More than the first line is added to the description as well.
        enum AccountStatus {
            /// When user is valid to login, these enum variant level docs are omitted!!!!!
            /// Since the OpenAPI spec does not have a place to put such information.
            Enabled,
            /// Login failed too many times
            Locked,
            Disabled
        }
    };

    assert_value! {account=>
        "description" = r#""This is user account status enum\n\nDetailed documentation here.\nMore than the first line is added to the description as well.""#, "AccountStatus description"
    }
}

#[test]
fn derive_struct_unnamed_field_single_value_type_success() {
    let point = api_doc! {
        struct Point(f32);
    };

    assert_value! {point=>
        "type" = r#""number""#, "Point type"
        "format" = r#""float""#, "Point format"
    }
}

#[test]
fn derive_struct_unnamed_fields_tuple_with_same_type_success() {
    let point = api_doc! {
        /// Contains x and y coordinates
        ///
        /// Coordinates are used to pinpoint location on a map
        struct Point(f64, f64);
    };

    assert_value! {point=>
        "type" = r#""array""#, "Point type"
        "items.type" = r#""number""#, "Point items type"
        "items.format" = r#""double""#, "Point items format"
        "items.description" = r#""Contains x and y coordinates\n\nCoordinates are used to pinpoint location on a map""#, "Point items description"
        "maxItems" = r#"2"#, "Wrapper max items"
        "minItems" = r#"2"#, "Wrapper min items"
    }
}

#[test]
fn derive_struct_unnamed_fields_tuple_with_different_types_success() {
    let point = api_doc! {
        struct Point(f64, String);
    };

    assert_value! {point=>
        "type" = r#""array""#, "Point type"
        "items.type" = r#""object""#, "Point items type"
        "items.format" = r#"null"#, "Point items format"
    }
}

#[test]
fn derive_struct_unnamed_field_with_generic_types_success() {
    let point = api_doc! {
        struct Wrapper(Option<String>);
    };

    assert_value! {point=>
        "type" = r#""string""#, "Wrapper type"
    }
}

#[test]
fn derive_struct_unnamed_field_with_nested_generic_type_success() {
    let point = api_doc! {
        /// Some description
        struct Wrapper(Option<Vec<i32>>);
    };

    assert_value! {point=>
        "type" = r#""array""#, "Wrapper type"
        "items.type" = r#""integer""#, "Wrapper items type"
        "items.format" = r#""int32""#, "Wrapper items format"
        "description" = r#""Some description""#, "Wrapper description"
    }
}

#[test]
fn derive_struct_unnamed_field_with_multiple_nested_generic_type_success() {
    let point = api_doc! {
        /// Some documentation
        struct Wrapper(Option<Vec<i32>>, String);
    };

    assert_value! {point=>
        "type" = r#""array""#, "Wrapper type"
        "items.type" = r#""object""#, "Wrapper items type"
        "items.format" = r#"null"#, "Wrapper items format"
        "description" = r#""Some documentation""#, "Wrapper description"
    }
}

#[test]
fn derive_struct_unnamed_field_vec_type_success() {
    let point = api_doc! {
        /// Some documentation
        /// more documentation
        struct Wrapper(Vec<i32>);
    };

    assert_value! {point=>
        "type" = r#""array""#, "Wrapper type"
        "items.type" = r#""integer""#, "Wrapper items type"
        "items.format" = r#""int32""#, "Wrapper items format"
        "maxItems" = r#"null"#, "Wrapper max items"
        "minItems" = r#"null"#, "Wrapper min items"
        "description" = r#""Some documentation\nmore documentation""#, "Wrapper description"
    }
}

#[test]
fn derive_struct_unnamed_field_single_value_default_success() {
    let point = api_doc! {
        #[schema(default)]
        struct Point(f32);

        impl Default for Point {
            fn default() -> Self {
                Self(3.5)
            }
        }
    };

    assert_value! {point=>
        "type" = r#""number""#, "Point type"
        "format" = r#""float""#, "Point format"
        "default" = r#"3.5"#, "Point default"
    }
}

#[test]
fn derive_struct_unnamed_field_multiple_value_default_ignored() {
    let point = api_doc! {
        #[schema(default)]
        struct Point(f32, f32);

        impl Default for Point {
            fn default() -> Self {
                Self(3.5, 6.4)
            }
        }
    };
    // Default values shouldn't be assigned as the struct is represented
    // as an array
    assert!(!point.to_string().contains("default"))
}

#[test]
fn derive_struct_nested_vec_success() {
    let vecs = api_doc! {
        struct VecTest {
            vecs: Vec<Vec<String>>
        }
    };

    assert_value! {vecs=>
        "properties.vecs.type" = r#""array""#, "Vecs property type"
        "properties.vecs.items.type" = r#""array""#, "Vecs property items type"
        "properties.vecs.items.items.type" = r#""string""#, "Vecs property items item type"
        "type" = r#""object""#, "Property type"
        "required.[0]" = r#""vecs""#, "Required properties"
    }
    common::assert_json_array_len(vecs.get("required").unwrap(), 1);
}

#[test]
fn derive_struct_with_example() {
    let pet = api_doc! {
        #[schema(example = json!({"name": "bob the cat", "age": 8}))]
        struct Pet {
            name: String,
            age: i32
        }
    };

    assert_value! {pet=>
        "example.name" = r#""bob the cat""#, "Pet example name"
        "example.age" = r#"8"#, "Pet example age"
    }
}

#[test]
fn derive_struct_with_deprecated() {
    #[allow(deprecated)]
    let pet = api_doc! {
        #[deprecated]
        struct Pet {
            name: String,
            #[deprecated]
            age: i32
        }
    };

    assert_value! {pet=>
        "deprecated" = r#"true"#, "Pet deprecated"
        "properties.name.type" = r#""string""#, "Pet properties name type"
        "properties.name.deprecated" = r#"null"#, "Pet properties name deprecated"
        "properties.age.type" = r#""integer""#, "Pet properties age type"
        "properties.age.deprecated" = r#"true"#, "Pet properties age deprecated"
        "example" = r#"null"#, "Pet example"
    }
}

#[test]
fn derive_struct_with_schema_deprecated() {
    let pet = api_doc! {
        #[schema(deprecated)]
        struct Pet {
            name: String,
            #[schema(deprecated)]
            age: i32
        }
    };

    assert_value! {pet=>
        "deprecated" = r#"true"#, "Pet deprecated"
        "properties.name.type" = r#""string""#, "Pet properties name type"
        "properties.name.deprecated" = r#"null"#, "Pet properties name deprecated"
        "properties.age.type" = r#""integer""#, "Pet properties age type"
        "properties.age.deprecated" = r#"true"#, "Pet properties age deprecated"
        "example" = r#"null"#, "Pet example"
    }
}

#[test]
fn derive_unnamed_struct_deprecated_success() {
    #[allow(deprecated)]
    let pet_age = api_doc! {
        #[deprecated]
        #[schema(example = 8)]
        struct PetAge(u64);
    };

    assert_value! {pet_age=>
        "deprecated" = r#"true"#, "PetAge deprecated"
        "example" = r#"8"#, "PetAge example"
    }
}

#[test]
fn derive_unnamed_struct_schema_deprecated_success() {
    let pet_age = api_doc! {
        #[schema(deprecated, example = 8)]
        struct PetAge(u64);
    };

    assert_value! {pet_age=>
        "deprecated" = r#"true"#, "PetAge deprecated"
        "example" = r#"8"#, "PetAge example"
    }
}

#[test]
fn derive_unnamed_struct_example_json_array_success() {
    let pet_age = api_doc! {
        #[schema(example = "0", default = i64::default)]
        struct PetAge(i64, i64);
    };

    assert_value! {pet_age=>
        "type" = r#""array""#, "PetAge type"
        "items.example" = r#""0""#, "PetAge example"
        "items.default" = r#"0"#, "PetAge default"
        "items.type" = r#""integer""#, "PetAge default"
        "items.format" = r#""int64""#, "PetAge default"
        "maxItems" = r#"2"#, "PetAge max items"
        "minItems" = r#"2"#, "PetAge min items"
    }
}

#[test]
fn derive_enum_with_deprecated() {
    #[allow(deprecated)]
    let mode = api_doc! {
        #[deprecated]
        enum Mode {
            Mode1, Mode2
        }
    };

    assert_value! {mode=>
        "enum" = r#"["Mode1","Mode2"]"#, "Mode enum variants"
        "type" = r#""string""#, "Mode type"
        "deprecated" = r#"true"#, "Mode deprecated"
    };
}

#[test]
fn derive_enum_with_schema_deprecated() {
    let mode = api_doc! {
        #[schema(deprecated)]
        enum Mode {
            Mode1, Mode2
        }
    };

    assert_value! {mode=>
        "enum" = r#"["Mode1","Mode2"]"#, "Mode enum variants"
        "type" = r#""string""#, "Mode type"
        "deprecated" = r#"true"#, "Mode deprecated"
    };
}

#[test]
fn derive_struct_with_generics() {
    #[allow(unused)]
    enum Type {
        Foo,
        Bar,
    }
    let status = api_doc! {
        struct Status<Type> {
            t: Type
        }
    };

    assert_value! {status=>
        "properties.t.$ref" = r###""#/components/schemas/Type""###, "Status t field"
    };
}

#[test]
fn derive_struct_with_lifetime_generics() {
    #[allow(unused)]
    let greeting = api_doc! {
        struct Greeting<'a> {
            greeting: &'a str
        }
    };

    assert_value! {greeting=>
        "properties.greeting.type" = r###""string""###, "Greeting greeting field type"
    };
}

#[test]
fn derive_struct_with_cow() {
    #[allow(unused)]
    let greeting = api_doc! {
        struct Greeting<'a> {
            greeting: Cow<'a, str>
        }
    };

    common::assert_json_array_len(greeting.get("required").unwrap(), 1);
    assert_value! {greeting=>
        "properties.greeting.type" = r###""string""###, "Greeting greeting field type"
        "required.[0]" = r###""greeting""###, "Greeting required"
    };
}

#[test]
fn derive_with_box_and_refcell() {
    #[allow(unused)]
    struct Foo {
        name: &'static str,
    }

    let greeting = api_doc! {
        struct Greeting {
            foo: Box<Foo>,
            ref_cell_foo: RefCell<Foo>
        }
    };

    common::assert_json_array_len(greeting.get("required").unwrap(), 2);
    assert_value! {greeting=>
        "properties.foo.$ref" = r###""#/components/schemas/Foo""###, "Greeting foo field"
        "properties.ref_cell_foo.$ref" = r###""#/components/schemas/Foo""###, "Greeting ref_cell_foo field"
        "required.0" = r###""foo""###, "Greeting required 0"
        "required.1" = r###""ref_cell_foo""###, "Greeting required 1"
    };
}

#[test]
fn derive_struct_with_inline() {
    #[derive(salvo_oapi::ToSchema)]
    #[allow(unused)]
    struct Foo {
        name: &'static str,
    }

    let greeting = api_doc! {
        struct Greeting {
            #[schema(inline)]
            foo1: Foo,
            #[schema(inline)]
            foo2: Option<Foo>,
            #[schema(inline)]
            foo3: Option<Box<Foo>>,
            #[schema(inline)]
            foo4: Vec<Foo>,
        }
    };

    assert_json_eq!(
        &greeting,
        json!({
            "properties": {
                "foo1": {
                    "properties": {
                        "name": {
                            "type": "string"
                        },
                    },
                    "required": [
                        "name"
                    ],
                    "type": "object"
                },
                "foo2": {
                    "nullable": true,
                    "allOf": [
                     {
                         "properties": {
                             "name": {
                                 "type": "string"
                             },
                         },
                         "required": [
                             "name"
                         ],
                         "type": "object"
                     }
                    ]
                },
                "foo3": {
                    "nullable": true,
                    "allOf": [
                    {
                        "properties": {
                            "name": {
                                "type": "string"
                            },
                        },
                        "required": [
                            "name"
                        ],
                        "type": "object"
                    }
                    ]
                },
                "foo4": {
                    "items": {
                        "properties": {
                            "name": {
                                "type": "string"
                            },
                        },
                        "required": [
                            "name"
                        ],
                        "type": "object"
                    },
                    "type": "array"
                },
            },
            "required": [
                "foo1",
                "foo4",
            ],
            "type": "object"
        })
    );
}

#[test]
fn derive_simple_enum() {
    let value: Value = api_doc! {
        #[derive(Serialize)]
        enum Bar {
            A,
            B,
            C,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "enum": [
                "A",
                "B",
                "C",
            ],
            "type": "string",
        })
    );
}

#[test]
fn derive_simple_enum_serde_tag() {
    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag")]
        enum Bar {
            A,
            B,
            C,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "A",
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "B",
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "C",
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
            ],
        })
    );
}

#[test]
fn derive_simple_enum_serde_untagged() {
    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Foo {
            One,
            Two,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "type": "object",
            "nullable": true,
            "default": null,
        })
    );
}

#[test]
fn derive_struct_unnamed_field_reference_with_comment() {
    #[derive(ToSchema, Serialize)]
    struct Bar {
        value: String,
    }

    let value = api_doc! {
        #[derive(Serialize)]
        /// Description should not apply to $ref that is created for inner type Bar
        struct Foo(Bar);
    };

    assert_json_eq!(
        value,
        json!({
          "$ref": "#/components/schemas/Bar"
        })
    );
}

/// Derive a complex enum with named and unnamed fields.
#[test]
fn derive_complex_unnamed_field_reference_with_comment() {
    #[derive(Serialize)]
    struct CommentedReference(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        enum EnumWithReference {
            /// This is comment which will not be added to the document
            /// since $ref cannot have comments
            UnnamedFieldWithCommentReference(CommentedReference),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "UnnamedFieldWithCommentReference": {
                            "$ref": "#/components/schemas/CommentedReference",
                        },
                    },
                    "required": ["UnnamedFieldWithCommentReference"],
                },
            ],
        })
    );
}

#[test]
fn derive_enum_with_unnamed_primitive_field_with_tag() {
    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag")]
        enum EnumWithReference {
            Value(String),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": ["Value"]
                        },
                    },
                    "required": ["tag"]
                },
            ],
            "discriminator": {
                "propertyName": "tag"
            }
        })
    );
}

#[test]
fn derive_complex_enum_with_schema_properties() {
    let value: Value = api_doc! {
        /// This is the description
        #[derive(Serialize)]
        #[schema(example = json!(EnumWithProperties::Variant2{name: String::from("foobar")}),
            default = json!(EnumWithProperties::Variant{id: String::from("1")}))]
        enum EnumWithProperties {
            Variant {
                id: String
            },
            Variant2{
                name: String
            }
        }
    };

    assert_json_eq!(
        value,
        json!({
            "description": "This is the description",
            "default": {
                "Variant": {
                    "id": "1"
                }
            },
            "example": {
                "Variant2": {
                    "name": "foobar"
                }
            },
            "oneOf": [
                {
                    "properties": {
                        "Variant": {
                            "properties": {
                                "id": {
                                    "type": "string"
                                }
                            },
                            "required": ["id"],
                            "type": "object"
                        }
                    },
                    "required": ["Variant"],
                    "type": "object"
                },
                {
                    "properties": {
                        "Variant2": {
                            "properties": {
                                "name": {
                                    "type": "string"
                                }
                            },
                            "required": ["name"],
                            "type": "object"
                        }
                    },
                    "required": ["Variant2"],
                    "type": "object"
                }
            ]
        })
    )
}

// TODO fixme https://github.com/juhaku/salvo_oapi/issues/285#issuecomment-1249625860
#[test]
fn derive_enum_with_unnamed_single_field_with_tag() {
    #[derive(Serialize)]
    struct ReferenceValue(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "enum")]
        enum EnumWithReference {
            Value(ReferenceValue),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ReferenceValue",
                        },
                        {
                            "type": "object",
                            "properties": {
                                "enum": {
                                    "type": "string",
                                    "enum": ["Value"]

                                },
                            },
                            "required": ["enum"]
                        },
                    ],
                }
            ],
            "discriminator": {
                "propertyName": "enum"
            }
        })
    );
}

#[test]
fn derive_enum_with_named_fields_with_reference_with_tag() {
    #[derive(Serialize)]
    struct ReferenceValue(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "enum")]
        enum EnumWithReference {
            Value {
                field: ReferenceValue,
                a: String
            },
            UnnamedValue(ReferenceValue),
            UnitValue,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                        "a": {
                            "type": "string"
                        },
                        "enum": {
                            "enum": [
                                "Value"
                            ],
                            "type": "string"
                        },
                        "field": {
                            "$ref": "#/components/schemas/ReferenceValue"
                        }
                    },
                    "required": [
                        "field",
                        "a",
                        "enum"
                    ],
                    "type": "object"
                },
                {
                    "allOf": [
                        {
                        "$ref": "#/components/schemas/ReferenceValue",
                        },
                        {
                            "type": "object",
                            "properties": {
                                "enum": {
                                    "type": "string",
                                    "enum": ["UnnamedValue"]

                                },
                            },
                            "required": ["enum"]
                        }
                    ],
                },
                {
                    "properties": {
                        "enum": {
                            "enum": [
                                "UnitValue"
                            ],
                            "type": "string"
                        }
                    },
                    "required": [
                        "enum"
                    ],
                    "type": "object"
                }
            ],
            "discriminator": {
                "propertyName": "enum"
            }
        })
    );
}

/// Derive a complex enum with named and unnamed fields.
#[test]
fn derive_complex_enum() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        enum Bar {
            UnitValue,
            NamedFields {
                id: &'static str,
                names: Option<Vec<String>>
            },
            UnnamedFields(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "string",
                    "enum": [
                        "UnitValue",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "NamedFields": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                },
                                "names": {
                                    "type": "array",
                                    "nullable": true,
                                    "items": {
                                        "type": "string",
                                    },
                                },
                            },
                            "required": [
                                "id",
                            ],
                        },
                    },
                    "required": ["NamedFields"],
                },
                {
                    "type": "object",
                    "properties": {
                        "UnnamedFields": {
                            "$ref": "#/components/schemas/Foo",
                        },
                    },
                    "required": ["UnnamedFields"],
                },
            ],
        })
    );
}

#[test]
fn derive_complex_enum_title() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        enum Bar {
            #[schema(title = "Unit")]
            UnitValue,
            #[schema(title = "Named")]
            NamedFields {
                id: &'static str,
            },
            #[schema(title = "Unnamed")]
            UnnamedFields(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "string",
                    "title": "Unit",
                    "enum": [
                        "UnitValue",
                    ],
                },
                {
                    "type": "object",
                    "title": "Named",
                    "properties": {
                        "NamedFields": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                },
                            },
                            "required": [
                                "id",
                            ],
                        },
                    },
                    "required": ["NamedFields"],
                },
                {
                    "type": "object",
                    "title": "Unnamed",
                    "properties": {
                        "UnnamedFields": {
                            "$ref": "#/components/schemas/Foo",
                        },
                    },
                    "required": ["UnnamedFields"]
                },
            ],
        })
    );
}

#[test]
fn derive_complex_enum_example() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        enum EnumWithExample {
            #[schema(example = "EX: Unit")]
            UnitValue,
            #[schema(example = "EX: Named")]
            NamedFields {
                #[schema(example = "EX: Named id field")]
                id: &'static str,
            },
            #[schema(example = "EX: Unnamed")]
            UnnamedFields(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "string",
                    "example": "EX: Unit",
                    "enum": [
                        "UnitValue",
                    ],
                },
                {
                    "type": "object",
                    "example": "EX: Named",
                    "properties": {
                        "NamedFields": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "example": "EX: Named id field",
                                },
                            },
                            "required": [
                                "id",
                            ],
                        },
                    },
                    "required": ["NamedFields"]
                },
                {
                    "type": "object",
                    "example": "EX: Unnamed",
                    "properties": {
                        "UnnamedFields": {
                            "$ref": "#/components/schemas/Foo",
                        },
                    },
                    "required": ["UnnamedFields"]
                },
            ],
        })
    );
}

#[test]
fn derive_complex_enum_serde_rename_all() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(rename_all = "snake_case")]
        enum Bar {
            UnitValue,
            NamedFields {
                id: &'static str,
                names: Option<Vec<String>>
            },
            UnnamedFields(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "string",
                    "enum": [
                        "unit_value",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "named_fields": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                },
                                "names": {
                                    "type": "array",
                                    "nullable": true,
                                    "items": {
                                        "type": "string",
                                    },
                                },
                            },
                            "required": [
                                "id",
                            ],
                        },
                    },
                    "required": ["named_fields"]
                },
                {
                    "type": "object",
                    "properties": {
                        "unnamed_fields": {
                            "$ref": "#/components/schemas/Foo",
                        },
                    },
                    "required": ["unnamed_fields"]
                },
            ],
        })
    );
}

#[test]
fn derive_complex_enum_serde_rename_variant() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        enum Bar {
            #[serde(rename = "renamed_unit_value")]
            UnitValue,
            #[serde(rename = "renamed_named_fields")]
            NamedFields {
                #[serde(rename = "renamed_id")]
                id: &'static str,
                #[serde(rename = "renamed_names")]
                names: Option<Vec<String>>
            },
            #[serde(rename = "renamed_unnamed_fields")]
            UnnamedFields(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "string",
                    "enum": [
                        "renamed_unit_value",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "renamed_named_fields": {
                            "type": "object",
                            "properties": {
                                "renamed_id": {
                                    "type": "string",
                                },
                                "renamed_names": {
                                    "type": "array",
                                    "nullable": true,
                                    "items": {
                                        "type": "string",
                                    },
                                },
                            },
                            "required": [
                                "renamed_id",
                            ],
                        },
                    },
                    "required": ["renamed_named_fields"]
                },
                {
                    "type": "object",
                    "properties": {
                        "renamed_unnamed_fields": {
                            "$ref": "#/components/schemas/Foo",
                        },
                    },
                    "required": ["renamed_unnamed_fields"]
                },
            ],
        })
    );
}

#[test]
fn derive_struct_custom_rename() {
    let value: Value = api_doc! {
        #[schema(rename_all = "SCREAMING-KEBAB-CASE")]
        struct Post {
            post_id: i64,
            created_at: i64,
            #[schema(rename = "post_comment")]
            comment: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "POST-ID": {
                    "type": "integer",
                    "format": "int64",
                },
                "CREATED-AT": {
                    "type": "integer",
                    "format": "int64",
                },
                "post_comment": {
                    "type": "string",
                },
            },
            "type": "object",
            "required": [
                "POST-ID",
                "CREATED-AT",
                "post_comment"
            ]
        })
    )
}

#[test]
fn derive_complex_enum_custom_rename() {
    let value: Value = api_doc! {
        #[schema(rename_all = "UPPERCASE")]
        enum PostType {
            NewPost(String),

            #[schema(rename = "update_post", rename_all = "PascalCase")]
            Update {
                post_id: i64,
                created_at: i64,
                #[schema(rename = "post_comment")]
                comment: String,
            },

            RandomValue {
                id: i64,
            },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                        "NEWPOST": {
                            "type": "string"
                        }
                    },
                    "required": ["NEWPOST"],
                    "type": "object",
                },
                {
                    "properties": {
                        "update_post": {
                            "properties": {
                                "PostId": {
                                    "type": "integer",
                                    "format": "int64",
                                },
                                "CreatedAt": {
                                    "type": "integer",
                                    "format": "int64",
                                },
                                "post_comment": {
                                    "type": "string",
                                },
                            },
                            "type": "object",
                            "required": [
                                "PostId",
                                "CreatedAt",
                                "post_comment"
                            ]
                        }
                    },
                    "required": ["update_post"],
                    "type": "object",
                },
                {
                    "properties": {
                        "RANDOMVALUE": {
                            "properties": {
                                "id": {
                                    "type": "integer",
                                    "format": "int64",
                                },
                            },
                            "type": "object",
                            "required": [
                                "id",
                            ]
                        }
                    },
                    "required": ["RANDOMVALUE"],
                    "type": "object",
                }
            ]
        })
    )
}

#[test]
fn derive_complex_enum_use_serde_rename_over_custom_rename() {
    let value: Value = api_doc! {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        #[schema(rename_all = "UPPERCASE")]
        enum Random {
            #[serde(rename = "string_value")]
            #[schema(rename = "custom_value")]
            String(String),

            Number {
                id: i32,
            }
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                        "string_value": {
                            "type": "string",
                        },
                    },
                    "type": "object",
                    "required": ["string_value"]
                },
                {
                    "properties": {
                        "number": {
                            "properties": {
                                "id": {
                                    "type": "integer",
                                    "format": "int32",
                                }
                            },
                            "type": "object",
                            "required": ["id"]
                        }
                    },
                    "type": "object",
                    "required": ["number"]
                }
            ]
        })
    )
}

#[test]
fn derive_struct_with_title() {
    let value: Value = api_doc! {
        #[schema(title = "Post")]
        struct Post {
            id: i64,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "id": {
                    "type": "integer",
                    "format": "int64",
                }
            },
            "title": "Post",
            "required": ["id"],
            "type": "object",
        })
    )
}

#[test]
fn derive_enum_with_title() {
    let value: Value = api_doc! {
        #[schema(title = "UserType")]
        enum UserType {
            Admin,
            Moderator,
            User,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "enum": ["Admin", "Moderator", "User"],
            "title": "UserType",
            "type": "string",
        })
    )
}

#[test]
fn derive_complex_enum_with_title() {
    let value: Value = api_doc! {
        enum UserType {
            #[schema(title = "admin")]
            Admin(String),
            #[schema(title = "moderator")]
            Moderator{id: i32},
            #[schema(title = "user")]
            User,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                        "Admin": {
                            "type": "string"
                        }
                    },
                    "title": "admin",
                    "type": "object",
                    "required": ["Admin"]
                },
                {
                    "properties": {
                        "Moderator": {
                            "properties": {
                                "id": {
                                    "type": "integer",
                                    "format": "int32",
                                }
                            },
                            "required": ["id"],
                            "type": "object",
                        }
                    },
                    "required": ["Moderator"],
                    "title": "moderator",
                    "type": "object",
                },
                {
                    "enum": ["User"],
                    "title": "user",
                    "type": "string"
                }
            ]
        })
    )
}

/// Derive a complex enum with the serde `tag` container attribute applied for internal tagging.
/// Note that tuple fields are not supported.
#[test]
fn derive_complex_enum_serde_tag() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag")]
        enum Bar {
            UnitValue,
            NamedFields {
                id: &'static str,
                names: Option<Vec<String>>
            },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "UnitValue",
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                        },
                        "names": {
                            "type": "array",
                            "nullable": true,
                            "items": {
                                "type": "string",
                            },
                        },
                        "tag": {
                            "type": "string",
                            "enum": [
                                "NamedFields",
                            ],
                        },
                    },
                    "required": [
                        "id",
                        "tag",
                    ],
                },
            ],
            "discriminator": {
                "propertyName": "tag"
            }
        })
    );
}

#[test]
fn derive_serde_flatten() {
    #[derive(Serialize)]
    struct Metadata {
        category: String,
        total: u64,
    }

    #[derive(Serialize)]
    struct Record {
        amount: i64,
        description: String,
        #[serde(flatten)]
        metadata: Metadata,
    }

    #[derive(Serialize)]
    struct Pagination {
        page: i64,
        next_page: i64,
        per_page: i64,
    }

    // Single flatten field
    let value: Value = api_doc! {
        #[derive(Serialize)]
        struct Record {
            amount: i64,
            description: String,
            #[serde(flatten)]
            metadata: Metadata,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "allOf": [
            {
                "$ref": "#/components/schemas/Metadata"
            },
            {
                "type": "object",
                "properties": {
                    "amount": {
                        "type": "integer",
                        "format": "int64"
                    },
                    "description": {
                        "type": "string",
                    },
                },
                "required": [
                    "amount",
                    "description"
                ],
            },
            ]
        })
    );

    // Multiple flatten fields, with field that contain flatten as well.
    // Record contain Metadata that is flatten as well, but it doesn't matter
    // here as the generated spec will reference to Record directly.
    let value: Value = api_doc! {
        #[derive(Serialize)]
        struct NamedFields {
            id: &'static str,
            #[serde(flatten)]
            record: Record,
            #[serde(flatten)]
            pagination: Pagination
        }
    };

    assert_json_eq!(
        value,
        json!({
            "allOf": [
            {
                "$ref": "#/components/schemas/Record"
            },
            {
                "$ref": "#/components/schemas/Pagination"
            },
            {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                    },
                },
                "required": [
                    "id",
                ],
            },
            ]
        })
    );
}

#[test]
fn derive_complex_enum_serde_untagged() {
    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Foo {
            Bar(i32),
            Baz(String),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "format": "int32",
                    "type": "integer",
                },
                {
                    "type": "string",
                },
            ],
        })
    );
}

#[test]
fn derive_complex_enum_with_ref_serde_untagged() {
    #[derive(Serialize)]
    struct Foo {
        name: String,
        age: u32,
    }

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Bar {
            Baz(i32),
            FooBar(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "format": "int32",
                    "type": "integer",
                },
                {
                    "$ref": "#/components/schemas/Foo",
                },
            ],
        })
    );
}

#[test]
fn derive_complex_enum_with_ref_serde_untagged_named_fields() {
    #[derive(Serialize, ToSchema)]
    struct Bar {
        name: String,
        age: u32,
    }

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Foo {
            One { n: i32 },
            Two { bar: Bar },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                      "n": {
                        "format": "int32",
                        "type": "integer"
                      }
                    },
                    "required": [
                      "n"
                    ],
                    "type": "object"
                },
                {
                    "properties": {
                      "bar": {
                        "$ref": "#/components/schemas/Bar"
                      }
                    },
                    "required": [
                      "bar"
                    ],
                    "type": "object"
                }
            ]
        })
    );
}

#[test]
fn derive_complex_enum_with_ref_serde_untagged_named_fields_rename_all() {
    #[derive(Serialize, ToSchema)]
    struct Bar {
        name: String,
        age: u32,
    }

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Foo {
            #[serde(rename_all = "camelCase")]
            One { some_number: i32 },
            #[serde(rename_all = "camelCase")]
            Two { some_bar: Bar },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                      "someNumber": {
                        "format": "int32",
                        "type": "integer"
                      }
                    },
                    "required": [
                      "someNumber"
                    ],
                    "type": "object"
                },
                {
                    "properties": {
                      "someBar": {
                        "$ref": "#/components/schemas/Bar"
                      }
                    },
                    "required": [
                      "someBar"
                    ],
                    "type": "object"
                }
            ]
        })
    );
}

#[test]
fn derive_complex_enum_serde_adjacently_tagged() {
    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag", content = "content")]
        enum Foo {
            Bar(i32),
            Baz(String),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "Bar",
                            ],
                        },
                        "content": {
                            "format": "int32",
                            "type": "integer",
                        },
                    },
                    "required": [
                        "tag",
                        "content"
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "Baz",
                            ]
                        },
                        "content": {
                            "type": "string",
                        },
                    },
                    "required": [
                        "tag",
                        "content"
                    ],
                },
            ],
            "discriminator": {
                "propertyName": "tag",
            },
        })
    );
}

#[test]
fn derive_complex_enum_with_ref_serde_adjacently_tagged() {
    #[derive(Serialize)]
    struct Foo {
        name: String,
        age: u32,
    }

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag", content = "content")]
        enum Bar {
            Baz(i32),
            FooBar(Foo),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "Baz",
                            ],
                        },
                        "content": {
                            "type": "integer",
                            "format": "int32",
                        },
                    },
                    "required": [
                        "tag",
                        "content"
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "FooBar",
                            ],
                        },
                        "content": {
                            "$ref": "#/components/schemas/Foo"
                        },
                    },
                    "required": [
                        "tag",
                        "content"
                    ],
                },
            ],
            "discriminator": {
                "propertyName": "tag",
            },
        })
    );
}

#[test]
fn derive_complex_enum_with_ref_serde_adjacently_tagged_named_fields() {
    #[derive(Serialize, ToSchema)]
    struct Bar {
        name: String,
        age: u32,
    }

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag", content = "content")]
        enum Foo {
            One { n: i32 },
            Two { bar: Bar },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "One",
                            ],
                        },
                        "content": {
                            "type": "object",
                            "properties": {
                                "n": {
                                    "format": "int32",
                                    "type": "integer",
                                },
                            },
                            "required": [
                                "n",
                            ],
                        },
                    },
                    "required": [
                      "tag",
                      "content"
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "Two",
                            ],
                        },
                        "content": {
                            "type": "object",
                            "properties": {
                                "bar": {
                                    "$ref": "#/components/schemas/Bar",
                                },
                            },
                            "required": [
                                "bar",
                            ],
                        },
                    },
                    "required": [
                      "tag",
                      "content",
                    ],
                },
            ],
            "discriminator": {
                "propertyName": "tag",
            },
        })
    );
}

#[test]
fn derive_complex_enum_with_ref_serde_adjacently_tagged_named_fields_rename_all() {
    #[derive(Serialize, ToSchema)]
    struct Bar {
        name: String,
        age: u32,
    }

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag", content = "content")]
        enum Foo {
            #[serde(rename_all = "camelCase")]
            One { some_number: i32 },
            #[serde(rename_all = "camelCase")]
            Two { some_bar: Bar },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "One",
                            ],
                        },
                        "content": {
                            "type": "object",
                            "properties": {
                                "someNumber": {
                                    "format": "int32",
                                    "type": "integer",
                                },
                            },
                            "required": [
                                "someNumber",
                            ],
                        },
                    },
                    "required": [
                      "tag",
                      "content"
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "Two",
                            ],
                        },
                        "content": {
                            "type": "object",
                            "properties": {
                                "someBar": {
                                    "$ref": "#/components/schemas/Bar",
                                },
                            },
                            "required": [
                                "someBar",
                            ],
                        },
                    },
                    "required": [
                      "tag",
                      "content",
                    ],
                },
            ],
            "discriminator": {
                "propertyName": "tag",
            },
        })
    );
}

#[test]
fn derive_complex_enum_serde_tag_title() {
    #[derive(Serialize)]
    struct Foo(String);

    let value: Value = api_doc! {
        #[derive(Serialize)]
        #[serde(tag = "tag")]
        enum Bar {
            #[schema(title = "Unit")]
            UnitValue,
            #[schema(title = "Named")]
            NamedFields {
                id: &'static str,
            },
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "title": "Unit",
                    "properties": {
                        "tag": {
                            "type": "string",
                            "enum": [
                                "UnitValue",
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
                {
                    "type": "object",
                    "title": "Named",
                    "properties": {
                        "id": {
                            "type": "string",
                        },
                        "tag": {
                            "type": "string",
                            "enum": [
                                "NamedFields",
                            ],
                        },
                    },
                    "required": [
                        "id",
                        "tag",
                    ],
                },
            ],
            "discriminator": {
                "propertyName": "tag"
            }
        })
    );
}

#[test]
fn derive_struct_with_read_only_and_write_only() {
    let user = api_doc! {
        struct User {
            #[schema(read_only)]
            username: String,
            #[schema(write_only)]
            password: String
        }
    };

    assert_value! {user=>
        "properties.password.type" = r###""string""###, "User password type"
        "properties.password.writeOnly" = r###"true"###, "User password write only"
        "properties.password.readOnly" = r###"null"###, "User password read only"
        "properties.username.type" = r###""string""###, "User username type"
        "properties.username.readOnly" = r###"true"###, "User username read only"
        "properties.username.writeOnly" = r###"null"###, "User username write only"
    }
}

#[test]
fn derive_struct_with_nullable_and_required() {
    let user = api_doc! {
        #[derive(Serialize)]
        struct User {
            #[schema(nullable)]
            #[serde(with = "::serde_with::rust::double_option")]
            fax: Option<Option<String>>,
            #[schema(nullable)]
            phone: Option<Option<String>>,
            #[schema(nullable = false)]
            email: String,
            name: String,
            #[schema(nullable)]
            edit_history: Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            friends: Vec<Option<String>>,
            #[schema(required)]
            updated: Option<String>,
        }
    };

    assert_json_eq!(
        user,
        json!({
            "properties": {
                "fax": {
                    "type": "string",
                    "nullable": true,
                },
                "phone": {
                    "type": "string",
                    "nullable": true,
                },
                "email": {
                    "type": "string",
                },
                "name": {
                    "type": "string",
                },
                "edit_history": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "nullable": true,
                },
                "friends": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "nullable": true,
                    },
                },
                "updated": {
                    "type": "string",
                    "nullable": true,
                }
            },
            "required": [
                "email",
                "name",
                "edit_history",
                "updated",
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_struct_xml() {
    let user = api_doc! {
        #[schema(xml(name = "user", prefix = "u", namespace = "https://mynamespace.test"))]
        struct User {
            #[schema(xml(attribute, prefix = "u"))]
            id: i64,
            #[schema(xml(name = "user_name", prefix = "u"))]
            username: String,
            #[schema(xml(wrapped(name = "linkList"), name = "link"))]
            links: Vec<String>,
            #[schema(xml(wrapped, name = "photo_url"))]
            photos_urls: Vec<String>
        }
    };

    assert_value! {user=>
        "xml.attribute" = r###"null"###, "User xml attribute"
        "xml.name" = r###""user""###, "User xml name"
        "xml.prefix" = r###""u""###, "User xml prefix"
        "xml.namespace" = r###""https://mynamespace.test""###, "User xml namespace"
        "properties.id.xml.attribute" = r###"true"###, "User id xml attribute"
        "properties.id.xml.name" = r###"null"###, "User id xml name"
        "properties.id.xml.prefix" = r###""u""###, "User id xml prefix"
        "properties.id.xml.namespace" = r###"null"###, "User id xml namespace"
        "properties.username.xml.attribute" = r###"null"###, "User username xml attribute"
        "properties.username.xml.name" = r###""user_name""###, "User username xml name"
        "properties.username.xml.prefix" = r###""u""###, "User username xml prefix"
        "properties.username.xml.namespace" = r###"null"###, "User username xml namespace"
        "properties.links.xml.attribute" = r###"null"###, "User links xml attribute"
        "properties.links.xml.name" = r###""linkList""###, "User links xml name"
        "properties.links.xml.prefix" = r###"null"###, "User links xml prefix"
        "properties.links.xml.namespace" = r###"null"###, "User links xml namespace"
        "properties.links.xml.wrapped" = r###"true"###, "User links xml wrapped"
        "properties.links.items.xml.attribute" = r###"null"###, "User links xml items attribute"
        "properties.links.items.xml.name" = r###""link""###, "User links xml items name"
        "properties.links.items.xml.prefix" = r###"null"###, "User links xml items prefix"
        "properties.links.items.xml.namespace" = r###"null"###, "User links xml items namespace"
        "properties.links.items.xml.wrapped" = r###"null"###, "User links xml items wrapped"
        "properties.photos_urls.xml.attribute" = r###"null"###, "User photos_urls xml attribute"
        "properties.photos_urls.xml.name" = r###"null"###, "User photos_urls xml name"
        "properties.photos_urls.xml.prefix" = r###"null"###, "User photos_urls xml prefix"
        "properties.photos_urls.xml.namespace" = r###"null"###, "User photos_urls xml namespace"
        "properties.photos_urls.xml.wrapped" = r###"true"###, "User photos_urls xml wrapped"
        "properties.photos_urls.items.xml.attribute" = r###"null"###, "User photos_urls xml items attribute"
        "properties.photos_urls.items.xml.name" = r###""photo_url""###, "User photos_urls xml items name"
        "properties.photos_urls.items.xml.prefix" = r###"null"###, "User photos_urls xml items prefix"
        "properties.photos_urls.items.xml.namespace" = r###"null"###, "User photos_urls xml items namespace"
        "properties.photos_urls.items.xml.wrapped" = r###"null"###, "User photos_urls links xml items wrapped"
    }
}

#[test]
fn derive_struct_xml_with_optional_vec() {
    let user = api_doc! {
        #[schema(xml(name = "user"))]
        struct User {
            #[schema(xml(attribute, prefix = "u"))]
            id: i64,
            #[schema(xml(wrapped(name = "linkList"), name = "link"))]
            links: Option<Vec<String>>,
        }
    };

    assert_json_eq!(
        user,
        json!({
            "properties": {
                "id": {
                    "type": "integer",
                    "format": "int64",
                    "xml": {
                        "attribute": true,
                        "prefix": "u"
                    }
                },
                "links": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "xml": {
                            "name": "link"
                        }
                    },
                    "nullable": true,
                    "xml": {
                        "name": "linkList",
                        "wrapped": true,
                    }
                }
            },
            "required": ["id"],
            "type": "object",
            "xml": {
                "name": "user"
            }
        })
    );
}

#[cfg(feature = "chrono")]
#[test]
fn derive_component_with_chrono_feature() {
    #![allow(deprecated)] // allow deprecated Date in tests as long as it is available from chrono
    use chrono::{Date, DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};

    let post = api_doc! {
        struct Post {
            id: i32,
            value: String,
            datetime: DateTime<Utc>,
            naive_datetime: NaiveDateTime,
            date: Date<Utc>,
            naive_date: NaiveDate,
            naive_time: NaiveTime,
            duration: Duration,
        }
    };

    assert_value! {post=>
        "properties.datetime.type" = r#""string""#, "Post datetime type"
        "properties.datetime.format" = r#""date-time""#, "Post datetime format"
        "properties.naive_datetime.type" = r#""string""#, "Post datetime type"
        "properties.naive_datetime.format" = r#""date-time""#, "Post datetime format"
        "properties.date.type" = r#""string""#, "Post date type"
        "properties.date.format" = r#""date""#, "Post date format"
        "properties.naive_date.type" = r#""string""#, "Post date type"
        "properties.naive_date.format" = r#""date""#, "Post date format"
        "properties.naive_time.type" = r#""string""#, "Post time type"
        "properties.naive_time.format" = r#"null"#, "Post time format"
        "properties.duration.type" = r#""string""#, "Post duration type"
        "properties.duration.format" = r#"null"#, "Post duration format"
        "properties.id.type" = r#""integer""#, "Post id type"
        "properties.id.format" = r#""int32""#, "Post id format"
        "properties.value.type" = r#""string""#, "Post value type"
        "properties.value.format" = r#"null"#, "Post value format"
    }
}

#[cfg(feature = "time")]
#[test]
fn derive_component_with_time_feature() {
    use time::{Date, Duration, OffsetDateTime, PrimitiveDateTime};

    let times = api_doc! {
        struct Timetest {
            datetime: OffsetDateTime,
            primitive_date_time: PrimitiveDateTime,
            date: Date,
            duration: Duration,
        }
    };

    assert_json_eq!(
        &times,
        json!({
            "properties": {
                "date": {
                    "format": "date",
                    "type": "string"
                },
                "datetime": {
                    "format": "date-time",
                    "type": "string"
                },
                "primitive_date_time": {
                    "format": "date-time",
                    "type": "string"
                },
                "duration": {
                    "type": "string"
                }
            },
            "required": [
                "datetime",
                "primitive_date_time",
                "date",
                "duration"
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_struct_component_field_type_override() {
    let post = api_doc! {
        struct Post {
            id: i32,
            #[schema(value_type = String)]
            value: i64,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""integer""#, "Post id type"
        "properties.id.format" = r#""int32""#, "Post id format"
        "properties.value.type" = r#""string""#, "Post value type"
        "properties.value.format" = r#"null"#, "Post value format"
    }
}

#[test]
fn derive_struct_component_field_type_path_override() {
    let post = api_doc! {
        struct Post {
            id: i32,
            #[schema(value_type = path::to::Foo)]
            value: i64,
        }
    };

    let component_ref: &str = post.pointer("/properties/value/$ref").unwrap().as_str().unwrap();
    assert_eq!(component_ref, "#/components/schemas/path.to.Foo");
}

#[test]
fn derive_struct_component_field_type_override_with_format() {
    let post = api_doc! {
        struct Post {
            id: i32,
            #[schema(value_type = String, format = Byte)]
            value: i64,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""integer""#, "Post id type"
        "properties.id.format" = r#""int32""#, "Post id format"
        "properties.value.type" = r#""string""#, "Post value type"
        "properties.value.format" = r#""byte""#, "Post value format"
    }
}

#[test]
fn derive_struct_component_field_type_override_with_custom_format() {
    let post = api_doc! {
        struct Post {
            #[schema(value_type = String, format = "uri")]
            value: String,
        }
    };

    assert_value! {post=>
        "properties.value.type" = r#""string""#, "Post value type"
        "properties.value.format" = r#""uri""#, "Post value format"
    }
}

#[test]
fn derive_struct_component_field_type_override_with_format_with_vec() {
    let post = api_doc! {
        struct Post {
            id: i32,
            #[schema(value_type = String, format = Binary)]
            value: Vec<u8>,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""integer""#, "Post id type"
        "properties.id.format" = r#""int32""#, "Post id format"
        "properties.value.type" = r#""string""#, "Post value type"
        "properties.value.format" = r#""binary""#, "Post value format"
    }
}

#[test]
fn derive_unnamed_struct_schema_type_override() {
    let value = api_doc! {
        #[schema(value_type = String)]
        struct Value(i64);
    };

    assert_value! {value=>
        "type" = r#""string""#, "Value type"
        "format" = r#"null"#, "Value format"
    }
}

#[test]
fn derive_unnamed_struct_schema_type_override_with_format() {
    let value = api_doc! {
        #[schema(value_type = String, format = Byte)]
        struct Value(i64);
    };

    assert_value! {value=>
        "type" = r#""string""#, "Value type"
        "format" = r#""byte""#, "Value format"
    }
}

#[test]
fn derive_struct_override_type_with_object_type() {
    let value = api_doc! {
        struct Value {
            #[schema(value_type = Object)]
            field: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "type": "object",
            "properties": {
                "field": {
                    "type": "object"
                }
            },
            "required": ["field"]
        })
    )
}

#[test]
fn derive_struct_override_type_with_a_reference() {
    mod custom {
        #[allow(dead_code)]
        struct NewBar;
    }

    let value = api_doc! {
        struct Value {
            #[schema(value_type = NewBar)]
            field: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "type": "object",
            "properties": {
                "field": {
                    "$ref": "#/components/schemas/NewBar"
                }
            },
            "required": ["field"]
        })
    )
}

#[cfg(feature = "decimal")]
#[test]
fn derive_struct_with_rust_decimal() {
    use rust_decimal::Decimal;

    let post = api_doc! {
        struct Post {
            id: i32,
            rating: Decimal,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""integer""#, "Post id type"
        "properties.id.format" = r#""int32""#, "Post id format"
        "properties.rating.type" = r#""string""#, "Post rating type"
        "properties.rating.format" = r#"null"#, "Post rating format"
    }
}

#[cfg(feature = "decimal")]
#[test]
fn derive_struct_with_rust_decimal_with_type_override() {
    use rust_decimal::Decimal;

    let post = api_doc! {
        struct Post {
            id: i32,
            #[schema(value_type = f64)]
            rating: Decimal,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""integer""#, "Post id type"
        "properties.id.format" = r#""int32""#, "Post id format"
        "properties.rating.type" = r#""number""#, "Post rating type"
        "properties.rating.format" = r#""double""#, "Post rating format"
    }
}

#[cfg(feature = "uuid")]
#[test]
fn derive_struct_with_uuid_type() {
    use uuid::Uuid;

    let post = api_doc! {
        struct Post {
            id: Uuid,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""string""#, "Post id type"
        "properties.id.format" = r#""uuid""#, "Post id format"
    }
}

#[cfg(feature = "ulid")]
#[test]
fn derive_struct_with_ulid_type() {
    use ulid::Ulid;

    let post = api_doc! {
        struct Post {
            id: Ulid,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""string""#, "Post id type"
        "properties.id.format" = r#""ulid""#, "Post id format"
    }
}

#[cfg(feature = "url")]
#[test]
fn derive_struct_with_url_type() {
    use url::Url;

    let post = api_doc! {
        struct Post {
            id: Url,
        }
    };

    assert_value! {post=>
        "properties.id.type" = r#""string""#, "Post id type"
        "properties.id.format" = r#""uri""#, "Post id format"
    }
}

#[test]
fn derive_parse_serde_field_attributes() {
    struct S;
    let post = api_doc! {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Post<S> {
            #[serde(rename = "uuid")]
            id: String,
            #[serde(skip)]
            _p: PhantomData<S>,
            #[serde(skip_serializing)]
            _p2: PhantomData<S>,
            long_field_num: i64,
        }
    };

    assert_json_eq!(
        post,
        json!({
            "properties": {
                "longFieldNum": {
                    "format": "int64",
                    "type": "integer"
                },
                "uuid": {
                    "type": "string"
                }
            },
            "required": [
                "uuid",
                "longFieldNum"
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_parse_serde_simple_enum_attributes() {
    let value = api_doc! {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        enum Value {
            A,
            B,
            #[serde(skip)]
            C,
        }
    };

    assert_value! {value=>
        "enum" = r#"["a","b"]"#, "Value enum variants"
    }
}

#[test]
fn derive_parse_serde_complex_enum() {
    #[derive(Serialize)]
    struct Foo;
    let complex_enum = api_doc! {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        enum Bar {
            UnitValue,
            #[serde(rename_all = "camelCase")]
            NamedFields {
                #[serde(rename = "id")]
                named_id: &'static str,
                name_list: Option<Vec<String>>
            },
            UnnamedFields(Foo),
            #[serde(skip)]
            Random,
        }
    };

    assert_value! {complex_enum=>
        "oneOf.[0].enum" = r#"["unitValue"]"#, "Unit value enum"
        "oneOf.[0].type" = r#""string""#, "Unit value type"

        "oneOf.[1].properties.namedFields.properties.id.type" = r#""string""#, "Named fields id type"
        "oneOf.[1].properties.namedFields.properties.nameList.type" = r#""array""#, "Named fields nameList type"
        "oneOf.[1].properties.namedFields.properties.nameList.items.type" = r#""string""#, "Named fields nameList items type"
        "oneOf.[1].properties.namedFields.required" = r#"["id"]"#, "Named fields required"

        "oneOf.[2].properties.unnamedFields.$ref" = r###""#/components/schemas/Foo""###, "Unnamed fields ref"
    }
}

#[test]
fn derive_component_with_generic_types_having_path_expression() {
    let ty = api_doc! {
        struct Bar {
            args: Vec<std::vec::Vec<std::string::String>>
        }
    };

    let args = ty.pointer("/properties/args").unwrap();

    assert_json_eq!(
        args,
        json!({
            "items": {
                "items": {
                    "type": "string"
                },
                "type": "array"
            },
            "type": "array"
        })
    );
}

#[test]
fn derive_component_with_aliases() {
    struct A;

    #[derive(Debug, OpenApi)]
    #[openapi(components(schemas(MyAlias)))]
    struct ApiDoc;

    #[derive(ToSchema)]
    #[aliases(MyAlias = Bar<A>)]
    struct Bar<R> {
        #[allow(dead_code)]
        bar: R,
    }

    let doc = ApiDoc::openapi();
    let doc_value = &serde_json::to_value(doc).unwrap();

    let value = doc_value.pointer("/components/schemas").unwrap();
    assert_value! {value=>
        "MyAlias.properties.bar.$ref" = r###""#/components/schemas/A""###, "MyAlias aliased property"
    }
}

#[test]
fn derive_complex_enum_as() {
    struct Foobar;

    #[derive(ToSchema)]
    #[schema(as = named::BarBar)]
    #[allow(unused)]
    enum BarBar {
        Foo { foo: Foobar },
    }

    #[derive(OpenApi)]
    #[openapi(components(schemas(BarBar)))]
    struct ApiDoc;

    let doc = serde_json::to_value(&ApiDoc::openapi()).unwrap();
    let value = doc
        .pointer("/components/schemas/named.BarBar")
        .expect("Should have BarBar named to named.BarBar");

    assert_json_eq!(
        &value,
        json!({
            "oneOf": [
            {
                "properties": {
                    "Foo": {
                        "properties": {
                            "foo": {
                                "$ref": "#/components/schemas/Foobar"
                            }
                        },
                        "required": ["foo"],
                        "type": "object"
                    }
                },
                "required": ["Foo"],
                "type": "object",
            }
            ]
        })
    )
}

#[test]
fn derive_component_with_primitive_aliases() {
    #[derive(Debug, OpenApi)]
    #[openapi(components(schemas(BarString, BarInt, Foo)))]
    struct ApiDoc;

    #[derive(ToSchema)]
    #[aliases(BarString = Bar<String>, BarInt = Bar<i32>)]
    struct Bar<R> {
        #[allow(dead_code)]
        bar: R,
    }
    #[derive(ToSchema)]
    struct Foo {
        #[allow(dead_code)]
        #[schema(value_type=BarString)]
        baz: Bar<String>,
    }

    let doc = ApiDoc::openapi();
    let doc_value = &serde_json::to_value(doc).unwrap();

    let value = doc_value.pointer("/components/schemas").unwrap();

    assert_json_eq!(
        value,
        json!({
            "BarString": {
                "properties": {
                    "bar": {
                        "type": "string"
                    }
                },
                "type": "object",
                "required": ["bar"]
            },
            "BarInt": {
                "properties": {
                    "bar": {
                        "type": "integer",
                        "format": "int32",
                    }
               },
                "type": "object",
                "required": ["bar"]
            },
            "Foo": {
                "properties": {
                    "baz": {
                        "$ref": "#/components/schemas/BarString",
                    }
               },
                "type": "object",
                "required": ["baz"]
            }
        })
    );
}

#[test]
fn derive_component_with_to_schema_value_type() {
    #[derive(ToSchema)]
    struct Foo {
        #[allow(unused)]
        value: String,
    }

    let doc = api_doc! {
        #[allow(unused)]
        struct Random {
            #[schema(value_type = i64)]
            id: String,
            #[schema(value_type = Object)]
            another_id: String,
            #[schema(value_type = Vec<Vec<String>>)]
            value1: Vec<i64>,
            #[schema(value_type = Vec<String>)]
            value2: Vec<i64>,
            #[schema(value_type = Option<String>)]
            value3: i64,
            #[schema(value_type = Option<Object>)]
            value4: i64,
            #[schema(value_type = Vec<Object>)]
            value5: i64,
            #[schema(value_type = Vec<Foo>)]
            value6: i64,
        }
    };

    assert_json_eq!(
        doc,
        json!({
            "properties": {
                "another_id": {
                    "type": "object"
                },
                "id": {
                    "type": "integer",
                    "format": "int64"
                },
                "value1": {
                    "items": {
                        "items": {
                            "type": "string"
                        },
                        "type": "array"
                    },
                    "type": "array"
                },
                "value2": {
                    "items": {
                        "type": "string"
                    },
                    "type": "array"
                },
                "value3": {
                    "type": "string",
                    "nullable": true,
                },
                "value4": {
                    "type": "object",
                    "nullable": true,
                },
                "value5": {
                    "items": {
                        "type": "object"
                    },
                    "type": "array"
                },
                "value6": {
                    "items": {
                        "$ref": "#/components/schemas/Foo"
                    },
                    "type": "array"
                }
            },
            "required": [
                "id",
                "another_id",
                "value1",
                "value2",
                "value5",
                "value6",
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_component_with_complex_enum_lifetimes() {
    #[derive(ToSchema)]
    struct Foo<'foo> {
        #[allow(unused)]
        field: &'foo str,
    }

    let doc = api_doc! {
        enum Bar<'bar> {
            A { foo: Foo<'bar> },
            B,
            C,
        }
    };

    assert_json_eq!(
        doc,
        json!({
            "oneOf": [
                {
                    "properties": {
                        "A": {
                            "properties": {
                                "foo": {
                                    "$ref": "#/components/schemas/Foo"
                                }
                            },
                            "required": ["foo"],
                            "type": "object"
                        },
                    },
                    "required": ["A"],
                    "type": "object"
                },
                {
                    "enum": ["B"],
                    "type": "string"
                },
                {
                    "enum": ["C"],
                    "type": "string"
                }
            ]
        })
    )
}

#[test]
fn derive_component_with_raw_identifier() {
    let doc = api_doc! {
        struct Bar {
            r#in: String
        }
    };

    assert_json_eq!(
        doc,
        json!({
            "properties": {
                "in": {
                    "type": "string"
                }
            },
            "required": ["in"],
            "type": "object"
        })
    )
}

#[test]
fn derive_component_with_linked_list() {
    use std::collections::LinkedList;

    let example_schema = api_doc! {
        struct ExampleSchema {
            values: LinkedList<f64>
        }
    };

    assert_json_eq!(
        example_schema,
        json!({
            "properties": {
                "values": {
                    "items": {
                        "type": "number",
                        "format": "double"
                    },
                    "type": "array"
                }
            },
            "required": ["values"],
            "type": "object"
        })
    )
}

#[test]
#[cfg(feature = "smallvec")]
fn derive_component_with_smallvec_feature() {
    use smallvec::SmallVec;

    let bar = api_doc! {
        struct Bar<'b> {
            links: SmallVec<[&'b str; 2]>
        }
    };

    assert_json_eq!(
        bar,
        json!({
            "properties": {
                "links": {
                    "items": {
                        "type": "string"
                    },
                    "type": "array",
                }
            },
            "required": ["links"],
            "type": "object"
        })
    )
}

#[test]
fn derive_schema_with_default_field() {
    let value = api_doc! {
        #[derive(serde::Deserialize)]
        struct MyValue {
            #[serde(default)]
            field: String
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "field": {
                    "type": "string"
                }
            },
            "type": "object"
        })
    )
}

#[test]
fn derive_schema_with_default_struct() {
    let value = api_doc! {
        #[derive(serde::Deserialize, Default)]
        #[serde(default)]
        struct MyValue {
            field: String
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "field": {
                    "type": "string",
                    "default": ""
                }
            },
            "type": "object"
        })
    )
}

#[test]
#[cfg(feature = "repr")]
fn derive_schema_for_repr_enum() {
    let value = api_doc! {
        #[derive(serde::Deserialize)]
        #[repr(i32)]
        #[schema(example = 1, default = 0)]
        enum ExitCode {
            Error  = -1,
            Ok     = 0,
            Unknow = 1,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "enum": [-1, 0, 1],
            "type": "integer",
            "default": 0,
            "example": 1,
        })
    );
}

#[test]
#[cfg(feature = "repr")]
fn derive_schema_for_tagged_repr_enum() {
    let value: Value = api_doc! {
        #[derive(serde::Deserialize, serde::Serialize)]
        #[serde(tag = "tag")]
        #[repr(u8)]
        enum TaggedEnum {
            One = 0,
            Two,
            Three,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "integer",
                            "enum": [
                                0,
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "integer",
                            "enum": [
                                1,
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
                {
                    "type": "object",
                    "properties": {
                        "tag": {
                            "type": "integer",
                            "enum": [
                                2,
                            ],
                        },
                    },
                    "required": [
                        "tag",
                    ],
                },
            ],
        })
    );
}

#[test]
#[cfg(feature = "repr")]
fn derive_schema_for_skipped_repr_enum() {
    let value: Value = api_doc! {
        #[derive(serde::Deserialize, serde::Serialize)]
        #[repr(i32)]
        enum SkippedEnum {
            Error  = -1,
            Ok     = 0,
            #[serde(skip)]
            Unknown = 1,
        }
    };

    assert_value! {value=>
        "enum" = r#"[-1,0]"#, "SkippedEnum enum variants"
        "type" = r#""integer""#, "SkippedEnum enum type"
    };
}

#[test]
#[cfg(feature = "repr")]
fn derive_repr_enum_with_with_custom_default_fn_success() {
    let mode = api_doc! {
        #[schema(default = repr_mode_default_fn)]
        #[repr(u16)]
        enum ReprDefaultMode {
            Mode1 = 0,
            Mode2
        }
    };

    assert_value! {mode=>
        "default" = r#"1"#, "ReprDefaultMode default"
        "enum" = r#"[0,1]"#, "ReprDefaultMode enum variants"
        "type" = r#""integer""#, "ReprDefaultMode type"
    };
    assert_value! {mode=>
        "example" = Value::Null, "ReprDefaultMode example"
    }
}

#[cfg(feature = "repr")]
fn repr_mode_default_fn() -> u16 {
    1
}

#[test]
#[cfg(feature = "repr")]
fn derive_repr_enum_with_with_custom_default_fn_and_example() {
    let mode = api_doc! {
        #[schema(default = repr_mode_default_fn, example = 1)]
        #[repr(u16)]
        enum ReprDefaultMode {
            Mode1 = 0,
            Mode2
        }
    };

    assert_value! {mode=>
        "default" = r#"1"#, "ReprDefaultMode default"
        "enum" = r#"[0,1]"#, "ReprDefaultMode enum variants"
        "type" = r#""integer""#, "ReprDefaultMode type"
        "example" = r#"1"#, "ReprDefaultMode example"
    };
}

#[test]
fn derive_struct_with_vec_field_with_example() {
    let post = api_doc! {
        struct Post {
            id: i32,
            #[schema(example = json!(["foobar", "barfoo"]))]
            value: Vec<String>,
        }
    };

    assert_json_eq!(
        post,
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "integer",
                    "format": "int32"
                },
                "value": {
                    "type": "array",
                    "example": ["foobar", "barfoo"],
                    "items": {
                        "type": "string"
                    }
                }
            },
            "required": ["id", "value"]
        })
    );
}

#[test]
fn derive_struct_field_with_example() {
    struct MyStruct;
    let doc = api_doc! {
        struct MyValue {
            #[schema(example = "test")]
            field1: String,
            #[schema(example = json!("test"))]
            field2: String,
            #[schema(example = json!({
                "key1": "value1"
            }))]
            field3: HashMap<String, String>,
            #[schema(example = json!({
                "key1": "value1"
            }))]
            field4: HashMap<String, MyStruct>
        }
    };

    assert_json_eq!(
        doc,
        json!({
            "properties": {
                "field1": {
                    "type": "string",
                    "example": "test"
                },
                "field2": {
                    "type": "string",
                    "example": "test"
                },
                "field3": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "string",
                    },
                    "example": {
                        "key1": "value1"
                    }
                },
                "field4": {
                    "type": "object",
                    "additionalProperties": {
                        "$ref": "#/components/schemas/MyStruct",
                    },
                    "example": {
                        "key1": "value1"
                    }
                }
            },
            "required": [
                "field1",
                "field2",
                "field3",
                "field4"
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_struct_with_self_reference() {
    let value = api_doc! {
        struct Item {
            id: String,
            previous: Box<Self>,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "id": {
                    "type": "string",
                },
                "previous": {
                    "$ref": "#/components/schemas/Item",
                },
            },
            "type": "object",
            "required": ["id", "previous"]
        })
    )
}

#[test]
fn derive_unnamed_struct_with_self_reference() {
    let value = api_doc! {
        struct Item(Box<Item>);
    };

    assert_json_eq!(
        value,
        json!({
            "$ref": "#/components/schemas/Item"
        })
    )
}

#[test]
fn derive_enum_with_self_reference() {
    let value = api_doc! {
        enum EnumValue {
            Item(Box<Self>),
            Item2 {
                value: Box<Self>
            }
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "properties": {
                        "Item": {
                            "$ref": "#/components/schemas/EnumValue"
                        }
                    },
                    "type": "object",
                    "required": ["Item"],
                },
                {
                    "properties": {
                        "Item2": {
                            "properties": {
                                "value": {
                                    "$ref": "#/components/schemas/EnumValue"
                                }
                            },
                            "required": ["value"],
                            "type": "object",
                        }
                    },
                    "required": ["Item2"],
                    "type": "object",
                }
            ]
        })
    )
}

#[test]
fn derive_struct_with_validation_fields() {
    let value = api_doc! {
        struct Item {
            #[schema(maximum = 10, minimum = 5, multiple_of = 2.5)]
            id: i32,

            #[schema(max_length = 10, min_length = 5, pattern = "[a-z]*")]
            value: String,

            #[schema(max_items = 5, min_items = 1, min_length = 1)]
            items: Vec<String>,

            unsigned: u16,

            #[schema(minimum = 2)]
            unsigned_value: u32,

        }
    };

    let config = Config::new(CompareMode::Strict).numeric_mode(NumericMode::AssumeFloat);

    #[cfg(feature = "non_strict_integers")]
    assert_json_matches!(
        value,
        json!({
            "properties": {
                "id": {
                    "format": "int32",
                    "type": "integer",
                    "maximum": 10.0,
                    "minimum": 5.0,
                    "multipleOf": 2.5,
                },
                "value": {
                    "type": "string",
                    "maxLength": 10,
                    "minLength": 5,
                    "pattern": "[a-z]*"
                },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "minLength": 1,
                    },
                    "maxItems": 5,
                    "minItems": 1,
                },
                "unsigned": {
                    "type": "integer",
                    "format": "uint16",
                    "minimum": 0.0
                },
                "unsigned_value": {
                    "type": "integer",
                    "format": "uint32",
                    "minimum": 2.0,
                }
            },
            "type": "object",
            "required": [
                "id",
                "value",
                "items",
                "unsigned",
                "unsigned_value"
            ]
        }),
        config
    );

    #[cfg(not(feature = "non_strict_integers"))]
    assert_json_matches!(
        value,
        json!({
            "properties": {
                "id": {
                    "format": "int32",
                    "type": "integer",
                    "maximum": 10.0,
                    "minimum": 5.0,
                    "multipleOf": 2.5,
                },
                "value": {
                    "type": "string",
                    "maxLength": 10,
                    "minLength": 5,
                    "pattern": "[a-z]*"
                },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "minLength": 1,
                    },
                    "maxItems": 5,
                    "minItems": 1,
                },
                "unsigned": {
                    "type": "integer",
                    "format": "int32",
                    "minimum": 0.0
                },
                "unsigned_value": {
                    "type": "integer",
                    "format": "int32",
                    "minimum": 2.0,
                }
            },
            "type": "object",
            "required": [
                "id",
                "value",
                "items",
                "unsigned",
                "unsigned_value"
            ]
        }),
        config
    );
}

#[test]
fn derive_schema_with_slice_and_array() {
    let value = api_doc! {
        struct Item<'a> {
            array: [&'a str; 10],
            slice: &'a [&'a str],
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "array": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                },
                "slice": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    }
                }
            },
            "required": [
                "array",
                "slice"
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_schema_multiple_serde_definitions() {
    let value = api_doc! {
        #[derive(serde::Deserialize)]
        struct Value {
            #[serde(default)]
            #[serde(rename = "ID")]
            id: String
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "ID": {
                    "type": "string",
                }
            },
            "type": "object",
        })
    );
}

#[test]
fn derive_schema_with_custom_field_with_schema() {
    fn custom_type() -> Object {
        ObjectBuilder::new()
            .schema_type(salvo_oapi::openapi::SchemaType::String)
            .format(Some(salvo_oapi::openapi::SchemaFormat::Custom("email".to_string())))
            .description(Some("this is the description"))
            .build()
    }
    let value = api_doc! {
        struct Value {
            #[schema(schema_with = custom_type)]
            id: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "id": {
                    "description": "this is the description",
                    "type": "string",
                    "format": "email"
                }
            },
            "type": "object"
        })
    )
}

#[test]
fn derive_unit_type() {
    let data = api_doc! {
        struct Data {
            unit_type: ()
        }
    };

    assert_json_eq!(
        data,
        json!({
            "type": "object",
            "required": [ "unit_type" ],
            "properties": {
                "unit_type": {
                    "default": null,
                    "nullable": true
                }
            }
        })
    )
}

#[test]
fn derive_unit_struct_schema() {
    let value = api_doc! {
        struct UnitValue;
    };

    assert_json_eq!(
        value,
        json!({
            "nullable": true,
            "default": null,
        })
    )
}

#[test]
fn derive_schema_with_multiple_schema_attributes() {
    let value = api_doc! {
        struct UserName {
            #[schema(min_length = 5)]
            #[schema(max_length = 10)]
            name: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minLength": 5,
                    "maxLength": 10,
                }
            },
            "required": ["name"]
        })
    )
}

#[test]
fn derive_schema_with_generics_and_lifetimes() {
    struct TResult;

    let value = api_doc_aliases! {
        #[aliases(Paginated1 = Paginated<'b, String>, Paginated2 = Paginated<'b, Cow<'c, bool>>)]
        struct Paginated<'r, TResult> {
            pub total: usize,
            pub data: Vec<TResult>,
            pub next: Option<&'r str>,
            pub prev: Option<&'r str>,
        }
    };

    let config = Config::new(CompareMode::Strict).numeric_mode(NumericMode::AssumeFloat);

    assert_json_matches!(
        value,
        json!([
            [
                "Paginated1",
                {
                    "properties": {
                        "data": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            }
                    },
                    "next": {
                        "type": "string",
                        "nullable": true,
                    },
                    "prev": {
                        "type": "string",
                        "nullable": true,
                    },
                    "total": {
                        "type": "integer",
                        "minimum": 0.0,
                     }
                    },
                    "required": [
                        "total",
                        "data",
                    ],
                    "type": "object"
                }
            ],
            [
                "Paginated2",
                {
                    "properties": {
                        "data": {
                            "type": "array",
                            "items": {
                                "type": "boolean"
                            }
                        },
                        "next": {
                            "type": "string",
                            "nullable": true,
                        },
                        "prev": {
                            "type": "string",
                            "nullable": true,
                        },
                        "total": {
                            "type": "integer",
                            "minimum": 0.0
                        }
                    },
                    "required": [
                        "total",
                        "data",
                    ],
                    "type": "object"
                }
            ]
        ]),
        config
    )
}

#[test]
fn derive_struct_with_unit_alias() {
    struct V;

    let value = api_doc_aliases! {
        #[aliases(UnitDataValue = DataValue<TupleUnit>)]
        struct DataValue<V> {
            name: String,
            v: V,
        }
    };

    #[derive(salvo_oapi::OpenApi)]
    #[openapi(components(schemas(TupleUnit)))]
    struct ApiDoc;

    let doc = ApiDoc::openapi();
    let doc_value = serde_json::to_value(&doc).unwrap();
    let unit = doc_value.pointer("/components/schemas/TupleUnit").unwrap();

    assert_json_eq!(
        value,
        json!([[
              "UnitDataValue",
              {
                  "properties": {
                      "name": {
                          "type": "string",
                      },
                      "v": {
                          "$ref": "#/components/schemas/TupleUnit"
                      },
                  },
                  "required": ["name", "v"],
                  "type": "object"
              }
        ]])
    );

    assert_json_eq!(
        unit,
        json!({
            "default": null,
            "nullable": true,
        })
    );
}

#[test]
fn derive_struct_with_deprecated_fields() {
    struct Foobar;
    let account = api_doc! {
        struct Account {
            #[deprecated]
            id: i64,
            #[deprecated]
            username: String,
            #[deprecated]
            role_ids: Vec<i32>,
            #[deprecated]
            foobars: Vec<Foobar>,
            #[deprecated]
            map: HashMap<String, String>
        }
    };

    assert_json_eq!(
        account,
        json!({
            "properties": {
                "id": {
                    "type": "integer",
                    "format": "int64",
                    "deprecated": true
                },
                "username": {
                    "type": "string",
                    "deprecated": true
                },
                "role_ids": {
                    "type": "array",
                    "deprecated": true,
                    "items": {
                        "type": "integer",
                        "format": "int32"
                    }
                },
                "foobars": {
                    "type": "array",
                    "deprecated": true,
                    "items": {
                        "$ref": "#/components/schemas/Foobar"
                    }
                },
                "map": {
                    "additionalProperties": {
                        "type": "string"
                    },
                    "deprecated": true,
                    "type": "object"
                }
            },
            "required": ["id", "username", "role_ids", "foobars", "map"],
            "type": "object"
        })
    )
}

#[test]
fn derive_struct_with_schema_deprecated_fields() {
    struct Foobar;
    let account = api_doc! {
        struct AccountA {
            #[schema(deprecated)]
            id: i64,
            #[schema(deprecated)]
            username: String,
            #[schema(deprecated)]
            role_ids: Vec<i32>,
            #[schema(deprecated)]
            foobars: Vec<Foobar>,
            #[schema(deprecated)]
            map: HashMap<String, String>
        }
    };

    assert_json_eq!(
        account,
        json!({
            "properties": {
                "id": {
                    "type": "integer",
                    "format": "int64",
                    "deprecated": true
                },
                "username": {
                    "type": "string",
                    "deprecated": true
                },
                "role_ids": {
                    "type": "array",
                    "deprecated": true,
                    "items": {
                        "type": "integer",
                        "format": "int32"
                    }
                },
                "foobars": {
                    "type": "array",
                    "deprecated": true,
                    "items": {
                        "$ref": "#/components/schemas/Foobar"
                    }
                },
                "map": {
                    "additionalProperties": {
                        "type": "string"
                    },
                    "deprecated": true,
                    "type": "object"
                }
            },
            "required": ["id", "username", "role_ids", "foobars", "map"],
            "type": "object"
        })
    )
}

#[test]
fn derive_schema_with_object_type_description() {
    let value = api_doc! {
        struct Value {
            /// This is object value
            #[schema(value_type = Object)]
            object: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "object": {
                    "description": "This is object value",
                    "type": "object"
                },
            },
            "required": ["object"],
            "type": "object"
        })
    )
}

#[test]
fn derive_schema_with_explicit_value_type() {
    let value = api_doc! {
        struct Value {
            #[schema(value_type = Value)]
            any: String,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "any": {
                },
            },
            "required": ["any"],
            "type": "object"
        })
    )
}

#[test]
fn derive_schema_with_implicit_value_type() {
    let value = api_doc! {
        struct Value {
            any: serde_json::Value,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "any": {
                },
            },
            "required": ["any"],
            "type": "object"
        })
    )
}

#[test]
fn derive_tuple_named_struct_field() {
    #[derive(ToSchema)]
    #[allow(unused)]
    struct Person {
        name: String,
    }

    let value = api_doc! {
        struct Post {
            info: (String, i64, bool, Person)
        }
    };
    assert_json_eq!(
        value,
        json!({
            "properties": {
                "info": {
                    "items": {
                        "allOf": [
                            {
                                "type": "string"
                            },
                            {
                                "type": "integer",
                                "format": "int64",
                            },
                            {
                                "type": "boolean",
                            },
                            {
                                "$ref": "#/components/schemas/Person"
                            }
                        ]
                    },
                    "type": "array"
                }
            },
            "type": "object",
            "required": ["info"]
        })
    )
}

#[test]
fn derive_nullable_tuple() {
    let value = api_doc! {
        struct Post {
            /// This is description
            #[deprecated]
            info: Option<(String, i64)>
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "info": {
                    "items": {
                        "allOf": [
                            {
                                "type": "string"
                            },
                            {
                                "type": "integer",
                                "format": "int64",
                            },
                        ]
                    },
                    "type": "array",
                    "nullable": true,
                    "deprecated": true,
                    "description": "This is description",
                }
            },
            "type": "object",
        })
    )
}

#[test]
fn derive_unit_type_untagged_enum() {
    #[derive(Serialize)]
    struct AggregationRequest;

    let value = api_doc! {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum ComputeRequest {
            Aggregation(AggregationRequest),
            Breakdown,
        }
    };

    assert_json_eq!(
        value,
        json!({
            "oneOf": [
                {
                    "$ref": "#/components/schemas/AggregationRequest"
                },
                {
                    "type": "object",
                    "nullable": true,
                    "default": null,
                }
            ]
        })
    )
}

#[test]
fn derive_schema_with_unit_hashmap() {
    let value = api_doc! {
        struct Container {
            volumes: HashMap<String, HashMap<(), ()>>
        }
    };

    assert_json_eq!(
        value,
        json!({
            "properties": {
                "volumes": {
                    "additionalProperties": {
                        "additionalProperties": {
                            "default": null,
                            "nullable": true,
                        },
                        "type": "object"
                    },
                    "type": "object"
                },
            },
            "required": [
                "volumes"
            ],
            "type": "object"
        })
    )
}

#[test]
#[cfg(feature = "rc_schema")]
fn derive_struct_with_arc() {
    use std::sync::Arc;

    let greeting = api_doc! {
        struct Greeting {
            greeting: Arc<String>
        }
    };

    assert_json_eq!(
        greeting,
        json!({
            "properties": {
                "greeting": {
                    "type": "string"
                },
            },
            "required": [
                "greeting"
            ],
            "type": "object"
        })
    )
}

#[test]
#[cfg(feature = "rc_schema")]
fn derive_struct_with_nested_arc() {
    use std::sync::Arc;

    let greeting = api_doc! {
        struct Greeting {
            #[allow(clippy::redundant_allocation)]
            greeting: Arc<Arc<String>>
        }
    };

    assert_json_eq!(
        greeting,
        json!({
            "properties": {
                "greeting": {
                    "type": "string"
                },
            },
            "required": [
                "greeting"
            ],
            "type": "object"
        })
    )
}

#[test]
#[cfg(feature = "rc_schema")]
fn derive_struct_with_collection_of_arcs() {
    use std::sync::Arc;

    let greeting = api_doc! {
        struct Greeting {
            greeting: Arc<Vec<String>>
        }
    };

    assert_json_eq!(
        greeting,
        json!({
            "properties": {
                "greeting": {
                    "items": {
                        "type": "string",
                    },
                    "type": "array"
                },
            },
            "required": [
                "greeting"
            ],
            "type": "object"
        })
    )
}

#[test]
#[cfg(feature = "rc_schema")]
fn derive_struct_with_rc() {
    use std::rc::Rc;

    let greeting = api_doc! {
        struct Greeting {
            greeting: Rc<String>
        }
    };

    assert_json_eq!(
        greeting,
        json!({
            "properties": {
                "greeting": {
                    "type": "string"
                },
            },
            "required": [
                "greeting"
            ],
            "type": "object"
        })
    )
}

#[test]
fn derive_doc_hidden() {
    let map = api_doc! {
        #[doc(hidden)]
        struct Map {
            map: HashMap<String, String>,
        }
    };

    assert_value! { map=>
        "properties.map.additionalProperties.type" = r#""string""#, "Additional Property Type"
    };
}

#[test]
fn derive_schema_with_docstring_on_unit_varian_of_enum() {
    let value: Value = api_doc! {
        /// top level doc for My enum
        #[derive(Serialize)]
        enum MyEnum {
            /// unit variant doc
            UnitVariant,
            /// non-unit doc
            NonUnitVariant(String),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "description": "top level doc for My enum",
            "oneOf": [
                {
                    "description": "unit variant doc",
                    "enum": [
                        "UnitVariant"
                    ],
                    "type": "string"
                },
                {
                    "properties": {
                        "NonUnitVariant": {
                            "description": "non-unit doc",
                            "type": "string"
                        }
                    },
                    "required": [
                        "NonUnitVariant"
                    ],
                    "type": "object"
                }
            ]
        })
    );
}

#[test]
fn derive_schema_with_docstring_on_tuple_variant_first_element_option() {
    let value: Value = api_doc! {
        /// top level doc for My enum
        enum MyEnum {
            /// doc for tuple variant with Option as first element - I now produce a description
            TupleVariantWithOptionFirst(Option<String>),

            /// doc for tuple variant without Option as first element - I produce a description
            TupleVariantWithNoOption(String),
        }
    };

    assert_json_eq!(
        value,
        json!(
                {
                  "oneOf": [
                    {
                      "type": "object",
                      "required": [
                        "TupleVariantWithOptionFirst"
                      ],
                      "properties": {
                        "TupleVariantWithOptionFirst": {
                          "type": "string",
                          "nullable": true,
                          "description": "doc for tuple variant with Option as first element - I now produce a description"
                        }
                      }
                    },
                    {
                      "type": "object",
                      "required": [
                        "TupleVariantWithNoOption"
                      ],
                      "properties": {
                        "TupleVariantWithNoOption": {
                          "type": "string",
                          "description": "doc for tuple variant without Option as first element - I produce a description"
                        }
                      }
                    }
                  ],
                  "description": "top level doc for My enum"
                }
        )
    );

    let value: Value = api_doc! {
        /// top level doc for My enum
        enum MyEnum {
            /// doc for tuple variant with Option as first element - I now produce a description
            TupleVariantWithOptionFirst(Option<String>, String),

            /// doc for tuple variant without Option as first element - I produce a description
            TupleVariantWithOptionSecond(String, Option<String>),
        }
    };

    assert_json_eq!(
        value,
        json!({
            "description": "top level doc for My enum",
            "oneOf": [
                {
                    "properties": {
                        "TupleVariantWithOptionFirst": {
                            "description": "doc for tuple variant with Option as first element - I now produce a description",
                            "items": {
                                "type": "object"
                            },
                            "maxItems": 2,
                            "minItems": 2,
                            "type": "array"
                        }
                    },
                    "required": [
                        "TupleVariantWithOptionFirst"
                    ],
                    "type": "object"
                },
                {
                    "properties": {
                        "TupleVariantWithOptionSecond": {
                            "description": "doc for tuple variant without Option as first element - I produce a description",
                            "items": {
                                "type": "object"
                            },
                            "maxItems": 2,
                            "minItems": 2,
                            "type": "array"
                        }
                    },
                    "required": [
                        "TupleVariantWithOptionSecond"
                    ],
                    "type": "object"
                }
            ]
        })
    );
}
