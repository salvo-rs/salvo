//! Data Result schemas use serde tags without sharing HTTP error components.
use salvo_core::http::StatusError;
use salvo_oapi::{BasicType, Components, ComposeSchema, Object, RefOr, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
struct Wrapped {
    result: Result<String, i32>,
}

fn resolve(schema: RefOr<salvo_oapi::Schema>, components: &Components) -> Value {
    let value = json!(schema);
    let document = json!({"components":components});
    let mut schema = value
        .get("$ref")
        .and_then(Value::as_str)
        .map(|reference| document.pointer(&reference[1..]).unwrap().clone())
        .unwrap_or(value);
    schema["components"] = document["components"].clone();
    schema
}

#[test]
fn result_values_keep_tags_and_do_not_pollute_status_error() {
    let mut components = Components::new();
    let strings = Result::<String, i32>::to_schema(&mut components);
    let bools = Result::<bool, u8>::to_schema(&mut components);
    assert_ne!(json!(strings), json!(bools));
    let status = StatusError::to_schema(&mut components);
    let actual = resolve(status, &components);
    let mut fresh = Components::new();
    let status = StatusError::to_schema(&mut fresh);
    assert_eq!(actual, resolve(status, &fresh));
    let native = resolve(strings, &components);
    assert_eq!(native["oneOf"][0]["required"], json!(["Ok"]));
    assert_eq!(native["oneOf"][1]["required"], json!(["Err"]));
    let composed = Result::<String, i32>::compose(&mut components, vec![]);
    let composed = resolve(composed, &components);
    let wrapped = Wrapped::to_schema(&mut components);
    let wrapped = resolve(wrapped, &components);
    let values = vec![Ok::<_, i32>("one".to_owned()), Err(7)];
    assert_eq!(json!(values), json!([{"Ok":"one"},{"Err":7}]));
    println!(
        "RESULT_CONTRACT={}",
        json!([
            {"schema":native,"values":values,"invalid":["one",7,{}, {"Ok":"one","extra":0},{"Ok":1},{"Err":"bad"}]},
            {"schema":composed,"values":values,"invalid":["one",7,{}, {"Ok":"one","Err":1}]},
            {"schema":wrapped,"values":values.into_iter().map(|result|Wrapped{result}).collect::<Vec<_>>(),"invalid":[{"result":"one"}]}
        ])
    );
}

#[test]
fn result_composition_uses_supplied_child_schemas_and_supports_borrows() {
    let schema = Result::<String, i32>::compose(
        &mut Components::new(),
        vec![
            Object::with_type(BasicType::Boolean).into(),
            Object::with_type(BasicType::Number).into(),
        ],
    );
    assert_eq!(
        json!(schema)["oneOf"][0]["properties"]["Ok"]["type"],
        "boolean"
    );
    fn borrowed<'a>(_: &'a str) {
        let _ = Result::<&'a str, i32>::to_schema(&mut Components::new());
    }
    borrowed(&String::from("borrowed"));
}
