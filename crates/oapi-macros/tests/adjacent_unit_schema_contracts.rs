//! Adjacent unit variants only serialize the tag, with the variant's wire name.
#![allow(dead_code)]
use salvo::oapi::{Components, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum Units {
    First,
    Second,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum Renamed {
    FirstChoice,
    #[serde(rename = "custom")]
    SecondChoice,
    #[serde(skip)]
    Hidden,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(content = "payload")]
#[serde(tag = "type")]
enum Split {
    Ready,
}

fn case<T: Serialize + ToSchema>(valid: Vec<T>, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components}, "valid":valid, "invalid":invalid})
}

#[test]
fn adjacent_unit_schemas_describe_real_serde_tags() {
    assert_eq!(json!(Units::First), json!({"kind":"First"}));
    assert!(serde_json::from_value::<Units>(json!({"kind":"First"})).is_ok());
    let cases = vec![
        case(
            vec![Units::First, Units::Second],
            vec![
                json!({}),
                json!({"kind":"data","data":"First"}),
                json!({"kind":"Other"}),
                json!("First"),
            ],
        ),
        case(
            vec![Renamed::FirstChoice, Renamed::SecondChoice],
            vec![json!({"kind":"Hidden"}), json!({"kind":"FirstChoice"})],
        ),
        case(
            vec![Split::Ready],
            vec![
                json!({"type":"payload","payload":"Ready"}),
                json!({"type":"Other"}),
            ],
        ),
    ];
    let mut components = Components::new();
    Units::to_schema(&mut components);
    let schema = json!(components);
    let variants = schema["schemas"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(variants[0]["properties"]["kind"]["enum"], json!(["First"]));
    assert_eq!(variants[0]["required"], json!(["kind"]));
    assert!(variants[0]["properties"].get("data").is_none());
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
