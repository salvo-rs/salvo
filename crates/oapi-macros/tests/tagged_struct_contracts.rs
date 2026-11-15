//! Struct tags belong to the object before flattening and closing its properties.
use std::collections::BTreeMap;

use salvo::oapi::{Components, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", deny_unknown_fields)]
struct Tagged {
    value: String,
}
#[derive(Serialize, ToSchema)]
#[serde(
    tag = "kind",
    rename(serialize = "wire-name", deserialize = "input-name"),
    deny_unknown_fields
)]
struct Renamed {
    value: String,
}
#[derive(Default, Serialize, ToSchema)]
struct Child {
    child: u32,
}
#[derive(Default, Serialize, ToSchema)]
#[serde(tag = "kind", default)]
struct Flattened {
    #[serde(flatten)]
    child: Child,
    #[serde(flatten)]
    extra: BTreeMap<String, u32>,
}

fn case<T: ToSchema + Serialize>(value: T, wrong_tag: Value) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":[wrong_tag]})
}
#[test]
fn tagged_structs_keep_the_serialized_type_name_and_flattened_properties() {
    let cases = vec![
        case(
            Tagged {
                value: "yes".into(),
            },
            json!({"value":"yes"}),
        ),
        case(
            Renamed {
                value: "yes".into(),
            },
            json!({"kind":"Renamed","value":"yes"}),
        ),
        case(
            Flattened {
                child: Child { child: 7 },
                extra: BTreeMap::from([("extra".into(), 8)]),
            },
            json!({"kind":"other","child":7,"extra":8}),
        ),
    ];
    for (case, expected) in cases.iter().zip(["Tagged", "wire-name", "Flattened"]) {
        assert_eq!(case["valid"][0]["kind"], expected);
        let schemas = &case["schema"]["components"]["schemas"];
        assert!(
            schemas
                .to_string()
                .contains(&format!("\"enum\":[\"{expected}\"]"))
        );
    }
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
