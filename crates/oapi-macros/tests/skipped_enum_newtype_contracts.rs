//! A skipped enum-newtype payload has unit style in every serde representation.
#![allow(dead_code)]
use salvo::oapi::{Components, ComposeSchema, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

struct NoSchema;
#[derive(Serialize, ToSchema)]
enum External<T> {
    #[serde(rename = "wire")]
    #[salvo(schema(title = "No payload", rename = "overridden", example = "wire"))]
    Entry(#[serde(skip)] T),
    SerializingSkip(#[serde(skip_serializing)] bool),
    InputOnly(#[serde(skip_deserializing)] String),
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Internal<T> {
    FirstEntry(#[serde(skip)] T),
    OtherEntry(#[serde(skip_serializing)] bool),
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum Adjacent<T> {
    #[salvo(schema(title = "No payload"))]
    Entry(#[serde(skip)] T),
    Other(#[serde(skip_serializing)] bool),
    Value(String),
}
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
enum Untagged<T> {
    Entry(#[serde(skip)] T),
    Other(#[serde(skip_serializing)] bool),
    Value(u32),
}
#[derive(Serialize, ToSchema)]
enum Fallback<T> {
    Entry,
    #[serde(untagged)]
    Fallback(#[serde(skip)] T),
}
#[derive(Serialize, ToSchema)]
struct SkippedStruct(#[serde(skip)] bool);

fn case<T: Serialize + ToSchema>(valid: Vec<T>, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components}, "valid":valid, "invalid":invalid})
}
#[test]
fn skipped_enum_newtype_fields_use_unit_shapes_without_schema_bounds() {
    assert_eq!(json!(External::Entry(NoSchema)), "wire");
    assert_eq!(
        json!(Internal::FirstEntry(NoSchema)),
        json!({"kind":"first_entry"})
    );
    assert_eq!(json!(Adjacent::Entry(NoSchema)), json!({"kind":"Entry"}));
    assert_eq!(json!(Untagged::Entry(NoSchema)), Value::Null);
    assert_eq!(json!(SkippedStruct(false)), false);
    let external = case(
        vec![
            External::Entry(NoSchema),
            External::SerializingSkip(false),
            External::InputOnly("x".into()),
        ],
        vec![
            json!({"wire":false}),
            json!({"SerializingSkip":false}),
            json!("InputOnly"),
        ],
    );
    let external_ref = external["schema"]["allOf"][0]["$ref"].as_str().unwrap();
    assert_eq!(
        external["schema"].pointer(&external_ref[1..]).unwrap()["oneOf"][0]["title"],
        "No payload"
    );
    let mut components = Components::new();
    let composed = External::<NoSchema>::compose(&mut components, vec![]);
    let cases = vec![
        external,
        case(
            vec![Internal::FirstEntry(NoSchema), Internal::OtherEntry(false)],
            vec![
                json!({"kind":"FirstEntry"}),
                json!({"kind":"Other"}),
                json!(false),
            ],
        ),
        case(
            vec![
                Adjacent::Entry(NoSchema),
                Adjacent::Other(false),
                Adjacent::Value("x".into()),
            ],
            vec![
                json!({"kind":"data","data":"Other"}),
                json!({"kind":"Unknown"}),
            ],
        ),
        case(
            vec![
                Untagged::Entry(NoSchema),
                Untagged::Other(false),
                Untagged::Value(7),
            ],
            vec![json!(false), json!([]), json!({"Entry":false})],
        ),
        case(
            vec![Fallback::Entry, Fallback::Fallback(NoSchema)],
            vec![json!({"Fallback":false}), json!(false)],
        ),
        case(vec![SkippedStruct(false)], vec![json!(null), json!([])]),
        json!({"schema":{"allOf":[composed],"components":components},"valid":["wire","SerializingSkip",{"InputOnly":"x"}],"invalid":[{"wire":false}]}),
    ];
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
