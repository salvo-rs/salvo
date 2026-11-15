//! Transparent wrappers serialize exactly like their selected field.
#![allow(dead_code)]
use std::marker::PhantomData;

use salvo::oapi::schema::BasicType;
use salvo::oapi::{Components, ComposeSchema, Object, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
#[serde(transparent)]
struct Named {
    value: String,
}
#[derive(Default, Serialize, ToSchema)]
#[serde(transparent)]
#[salvo(schema(default))]
struct Tuple(#[serde(skip)] bool, String);
#[derive(Serialize, ToSchema)]
#[serde(transparent)]
struct Generic<T, Marker> {
    #[serde(skip)]
    marker: PhantomData<Marker>,
    value: T,
}
struct NoSchema;

fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}
#[test]
fn transparent_wrappers_match_serde_and_generic_composition() {
    let cases = vec![
        case(
            Named {
                value: "yes".into(),
            },
            vec![json!({"value":"yes"})],
        ),
        case(Tuple(false, "yes".into()), vec![json!([false, "yes"])]),
        case(
            Generic::<String, NoSchema> {
                marker: PhantomData,
                value: "yes".into(),
            },
            vec![json!({"value":"yes"})],
        ),
    ];
    assert_eq!(
        cases[1]["schema"]["components"]["schemas"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()["default"],
        ""
    );
    let composed = json!(Generic::<String, NoSchema>::compose(
        &mut Components::new(),
        vec![
            Object::with_type(BasicType::Boolean).into(),
            Object::new().into()
        ]
    ));
    assert_eq!(composed["type"], "boolean");
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
