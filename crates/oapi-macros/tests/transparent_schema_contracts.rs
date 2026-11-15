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

#[derive(Serialize, ToSchema)]
#[serde(transparent)]
struct Constrained {
    /// The selected value.
    #[salvo(schema(min_length = 4, max_length = 8, example = "hello"))]
    value: String,
}

#[derive(Serialize, ToSchema)]
#[serde(transparent)]
struct TupleConstraint(#[salvo(schema(minimum = 3, maximum = 9))] u32);

#[derive(Serialize)]
#[serde(transparent)]
struct SerializedOnly(String);

#[derive(Serialize, ToSchema)]
#[serde(transparent)]
struct TypeOverride<T> {
    #[salvo(schema(value_type = String, min_length = 4))]
    value: T,
}

#[derive(Serialize, ToSchema)]
#[serde(transparent)]
struct CustomSchema<T>(#[salvo(schema(schema_with = custom_string))] T);

fn custom_string() -> Object {
    Object::with_type(BasicType::String).pattern("^custom")
}

#[derive(Default, Serialize, ToSchema)]
#[serde(transparent, default)]
struct SerdeDefault {
    value: u32,
}

#[derive(Serialize, ToSchema)]
#[serde(transparent, default = "factory_default")]
struct FactoryDefault {
    value: String,
}
fn factory_default() -> FactoryDefault {
    FactoryDefault {
        value: "factory".into(),
    }
}

#[derive(Serialize, ToSchema)]
#[serde(transparent, default = "tuple_factory")]
struct TupleFactory(#[serde(skip)] bool, String);
fn tuple_factory() -> TupleFactory {
    TupleFactory(false, "tuple factory".into())
}

#[derive(Default, Serialize, ToSchema)]
#[serde(transparent, default = "explicit_factory")]
#[salvo(schema(default))]
struct ExplicitTrait {
    value: String,
}
fn explicit_factory() -> ExplicitTrait {
    ExplicitTrait {
        value: "factory".into(),
    }
}

#[derive(Serialize, ToSchema)]
#[serde(transparent, default = "explicit_value_factory")]
struct ExplicitValue {
    #[salvo(schema(default = "explicit"))]
    value: String,
}
fn explicit_value_factory() -> ExplicitValue {
    ExplicitValue {
        value: "factory".into(),
    }
}

fn value_schema<T: ComposeSchema>() -> Value {
    json!(T::compose(&mut Components::new(), vec![]))
}

#[test]
fn transparent_values_retain_serde_defaults_and_schema_precedence() {
    assert_eq!(value_schema::<SerdeDefault>()["default"], 0);
    assert_eq!(value_schema::<FactoryDefault>()["default"], "factory");
    assert_eq!(value_schema::<TupleFactory>()["default"], "tuple factory");
    assert_eq!(value_schema::<ExplicitTrait>()["default"], "");
    assert_eq!(value_schema::<ExplicitValue>()["default"], "explicit");
}

#[test]
fn transparent_selected_fields_keep_constraints() {
    let mut components = Components::new();
    Constrained::to_schema(&mut components);
    TupleConstraint::to_schema(&mut components);
    let values = json!(components);
    let schemas = values["schemas"].as_object().unwrap();
    let named = schemas
        .iter()
        .find(|(name, _)| name.ends_with("Constrained"))
        .unwrap()
        .1;
    assert_eq!(named["minLength"], 4);
    assert_eq!(named["maxLength"], 8);
    assert_eq!(named["examples"], json!(["hello"]));
    assert_eq!(named["description"], "The selected value.");
    let tuple = schemas
        .iter()
        .find(|(name, _)| name.ends_with("TupleConstraint"))
        .unwrap()
        .1;
    assert_eq!(tuple["minimum"], 3);
    assert_eq!(tuple["maximum"], 9);
    let overridden = json!(TypeOverride::<SerializedOnly>::compose(
        &mut Components::new(),
        vec![]
    ));
    assert_eq!(overridden["type"], "string");
    assert_eq!(overridden["minLength"], 4);
    let custom = json!(CustomSchema::<SerializedOnly>::compose(
        &mut Components::new(),
        vec![]
    ));
    assert_eq!(custom["pattern"], "^custom");
}

fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}
#[test]
fn transparent_wrappers_match_serde_and_generic_composition() {
    let cases = vec![
        case(
            Constrained {
                value: "hello".into(),
            },
            vec![json!("a"), json!("overlong value")],
        ),
        case(TupleConstraint(5), vec![json!(2), json!(10)]),
        case(
            TypeOverride {
                value: SerializedOnly("hello".into()),
            },
            vec![json!("a")],
        ),
        case(
            CustomSchema(SerializedOnly("custom value".into())),
            vec![json!("wrong")],
        ),
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
        cases[5]["schema"]["components"]["schemas"]
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
