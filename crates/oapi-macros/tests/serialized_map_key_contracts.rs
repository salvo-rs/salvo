//! Map property names describe serde's string serialization of Rust keys.
use std::collections::BTreeMap;

use salvo::oapi::{Components, ComposeSchema, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
struct Derived {
    values: BTreeMap<i32, String>,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
enum Key {
    Alpha,
    Beta,
}
fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}
#[test]
fn serialized_map_names_match_serde() {
    let mut components = Components::new();
    let schema = json!(BTreeMap::<i32, String>::to_schema(&mut components));
    assert_eq!(schema["propertyNames"]["type"], "string");
    let cases = vec![
        case(
            BTreeMap::from([(-7_i32, "one".to_owned())]),
            vec![
                json!({"no":"one"}),
                json!({"7.5":"one"}),
                json!({"7":false}),
            ],
        ),
        case(
            BTreeMap::from([(7_u32, "one".to_owned())]),
            vec![json!({"-7":"one"})],
        ),
        case(
            BTreeMap::from([(true, "one".to_owned())]),
            vec![json!({"other":"one"})],
        ),
        case(
            BTreeMap::from([("seven".to_owned(), "one".to_owned())]),
            vec![json!({"seven":false})],
        ),
        case(
            Derived {
                values: BTreeMap::from([(7, "one".to_owned())]),
            },
            vec![json!({"values":{"7":false}})],
        ),
        case(
            BTreeMap::from([(Key::Alpha, "one".to_owned())]),
            vec![json!({"Other":"one"})],
        ),
        json!({"schema":BTreeMap::<i32,String>::compose(&mut components,vec![]),"valid":[{"7":"one"}],"invalid":[{"no":"one"},{"7":false}]}),
    ];
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
