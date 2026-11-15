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

fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}

#[test]
fn tuple_struct_positions_match_serde() {
    let cases = vec![
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
