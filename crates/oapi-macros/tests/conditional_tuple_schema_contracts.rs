//! Conditional skips change array lengths and shift every following field.
#![allow(dead_code)]
use salvo::oapi::schema::{BasicType, Schema};
use salvo::oapi::{Components, ComposeSchema, Object, RefOr, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
#[salvo(schema(title = "Conditional tuple", example = json!([7])))]
struct Mixed(
    #[serde(skip_serializing_if = "Option::is_none")] Option<String>,
    u32,
);
#[derive(Serialize, ToSchema)]
struct Multiple(
    #[serde(skip_serializing_if = "Option::is_none")] Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] Option<bool>,
    i64,
);
#[derive(Serialize, ToSchema)]
#[salvo(schema(format = "wire-text"))]
struct Formatted(
    #[serde(skip_serializing_if = "String::is_empty")] String,
    String,
);
#[derive(Serialize, ToSchema)]
struct Generic<T, Skipped>(
    #[serde(skip_serializing_if = "Option::is_none")] Option<T>,
    #[serde(skip)] Skipped,
    u32,
);
#[derive(Serialize, ToSchema)]
enum External {
    Entry(
        #[serde(skip_serializing_if = "Option::is_none")] Option<String>,
        u32,
    ),
}
macro_rules! homogeneous {
    ($($field:ty),*) => {
        #[derive(Serialize, ToSchema)]
        struct Homogeneous($(#[serde(skip_serializing_if = "Option::is_none")] $field),*);
    };
}
homogeneous!(
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>
);

struct NoSchema;
struct ComposeOnly;
impl ComposeSchema for ComposeOnly {
    fn compose(_: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        Object::with_type(BasicType::Boolean).into()
    }
}
fn case<T: Serialize + ToSchema>(valid: Vec<T>, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components}, "valid":valid, "invalid":invalid})
}
#[test]
fn conditional_tuple_schemas_match_all_serialized_subsequences() {
    let mut multiple = vec![];
    for mask in 0..8 {
        multiple.push(Multiple(
            (mask & 1 != 0).then(|| "x".into()),
            (mask & 2 != 0).then_some(7),
            (mask & 4 != 0).then_some(true),
            -3,
        ));
    }
    let homogeneous = case(
        vec![
            Homogeneous(
                None, None, None, None, None, None, None, None, None, None, None, None,
            ),
            Homogeneous(
                Some(1),
                Some(2),
                None,
                None,
                Some(3),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        ],
        vec![
            json!([false]),
            json!([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]),
        ],
    );
    let homogeneous_schema = homogeneous["schema"]["allOf"][0]["$ref"].as_str().unwrap();
    assert_eq!(
        homogeneous["schema"]
            .pointer(&homogeneous_schema[1..])
            .unwrap()["anyOf"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    let mut components = Components::new();
    let generic = ComposeOnly::compose(&mut components, vec![]);
    let composed = Generic::<ComposeOnly, NoSchema>::compose(&mut components, vec![generic]);
    Generic::<String, NoSchema>::to_schema(&mut components);
    let cases = vec![
        case(
            vec![Mixed(None, 7), Mixed(Some("x".into()), 7)],
            vec![json!([]), json!(["x"]), json!([7, 7])],
        ),
        case(
            multiple,
            vec![
                json!([-3, -3, -3]),
                json!([true, "x", -3]),
                json!([7, 7, -3]),
                json!([]),
            ],
        ),
        case(
            vec![
                External::Entry(None, 7),
                External::Entry(Some("x".into()), 7),
            ],
            vec![json!({"Entry":[7,7]})],
        ),
        homogeneous,
        case(
            vec![
                Formatted("".into(), "kept".into()),
                Formatted("first".into(), "kept".into()),
            ],
            vec![json!([7]), json!([])],
        ),
        json!({"schema":{"allOf":[composed],"components":components}, "valid":[[7],[true,7]], "invalid":[["wrong",7],[true,true,7]]}),
    ];
    let mixed_ref = cases[0]["schema"]["allOf"][0]["$ref"].as_str().unwrap();
    let mixed = cases[0]["schema"].pointer(&mixed_ref[1..]).unwrap();
    assert_eq!(mixed["title"], "Conditional tuple");
    assert_eq!(mixed["examples"], json!([[7]]));
    let formatted_ref = cases[4]["schema"]["allOf"][0]["$ref"].as_str().unwrap();
    let formatted = cases[4]["schema"].pointer(&formatted_ref[1..]).unwrap();
    assert_eq!(
        formatted["anyOf"][0]["prefixItems"][0]["format"],
        "wire-text"
    );
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
