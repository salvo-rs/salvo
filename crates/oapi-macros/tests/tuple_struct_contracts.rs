//! Tuple struct schemas preserve each serialized array position.
#![allow(dead_code)]
use salvo::oapi::{Components, ComposeSchema, Object, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
struct Mixed(String, u32);
#[derive(Serialize, ToSchema)]
struct Generic<T>(T, u32);
#[derive(Serialize, ToSchema)]
struct Homogeneous(String, String);

#[derive(Serialize, ToSchema)]
struct SkippedMiddle(String, #[serde(skip)] bool, u32);
#[derive(Serialize, ToSchema)]
struct OneRemaining(#[serde(skip_serializing)] bool, String);
#[derive(Serialize, ToSchema)]
struct AllSkipped(#[serde(skip)] bool, #[serde(skip)] u32);
#[derive(Serialize, ToSchema)]
struct SkippedNewtype(#[serde(skip)] bool);
#[derive(Serialize, ToSchema)]
struct InputOnlySkip(#[serde(skip_deserializing)] String, u32);
#[derive(Serialize, ToSchema)]
struct GenericSkipped<T>(String, #[serde(skip)] T);
struct NoSchema;

fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}

#[test]
fn tuple_struct_positions_match_serde() {
    let cases = vec![
        case(
            SkippedMiddle("one".into(), false, 7),
            vec![json!(["one", false, 7]), json!(["one"])],
        ),
        case(
            OneRemaining(false, "one".into()),
            vec![json!("one"), json!([false, "one"])],
        ),
        case(AllSkipped(false, 7), vec![json!([false, 7]), json!(null)]),
        case(SkippedNewtype(false), vec![json!(null), json!([])]),
        case(InputOnlySkip("one".into(), 7), vec![json!([7])]),
        case(
            GenericSkipped("one".into(), NoSchema),
            vec![json!(["one", null])],
        ),
        case(
            Mixed("one".into(), 7),
            vec![json!([7, "one"]), json!(["one"]), json!(["one", 7, 8])],
        ),
        case(Generic("one".to_owned(), 7), vec![json!([7, "one"])]),
        case(
            Homogeneous("one".into(), "two".into()),
            vec![json!(["one", 7])],
        ),
    ];
    let mut components = Components::new();
    let composed = json!(Generic::<String>::compose(
        &mut components,
        vec![Object::with_type(salvo::oapi::schema::BasicType::Boolean).into()]
    ));
    assert_eq!(composed["prefixItems"][0]["type"], "boolean");
    assert_eq!(composed["prefixItems"][1]["type"], "integer");
    assert_eq!(composed["minItems"], 2);
    assert_eq!(composed["maxItems"], 2);
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
