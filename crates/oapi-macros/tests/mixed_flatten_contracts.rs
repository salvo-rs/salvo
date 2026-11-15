//! Catch-all map constraints apply after all flattened object properties.
#![allow(dead_code)]
use salvo::oapi::{Components, ComposeSchema, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Serialize, Deserialize, ToSchema)]
struct Child {
    label: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct Mixed {
    id: bool,
    #[serde(flatten)]
    child: Child,
    #[serde(flatten)]
    extra: BTreeMap<String, u32>,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct Reversed {
    #[serde(flatten)]
    extra: BTreeMap<String, u32>,
    #[serde(flatten)]
    child: Child,
}
#[derive(Serialize, ToSchema)]
struct MapOnly {
    title: String,
    #[serde(flatten)]
    extra: BTreeMap<String, u32>,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Tagged {
    Item {
        #[serde(flatten)]
        child: Child,
        #[serde(flatten)]
        extra: BTreeMap<String, u32>,
    },
}
#[derive(Serialize, ToSchema)]
struct Generic<T> {
    #[serde(flatten)]
    child: T,
    #[serde(flatten)]
    extra: BTreeMap<String, u32>,
}
fn ignored() -> bool {
    true
}
#[derive(Serialize, ToSchema)]
struct IgnoredMap {
    #[serde(flatten)]
    child: Child,
    #[serde(flatten)]
    #[salvo(schema(ignore=ignored))]
    extra: BTreeMap<String, u32>,
}
fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}
#[test]
fn flattened_known_fields_are_exempt_from_map_value_rules() {
    let mixed = Mixed {
        id: true,
        child: Child {
            label: "yes".into(),
        },
        extra: BTreeMap::from([("count".into(), 2)]),
    };
    let _: Mixed = serde_json::from_value(json!(mixed)).unwrap();
    let mut cases = vec![
        case(
            mixed,
            vec![
                json!({"id":true,"label":false,"count":2}),
                json!({"id":true,"label":"yes","count":"bad"}),
                json!({"id":true,"count":2}),
            ],
        ),
        case(
            Reversed {
                child: Child {
                    label: "yes".into(),
                },
                extra: BTreeMap::from([("count".into(), 2)]),
            },
            vec![json!({"label":"yes","count":"bad"})],
        ),
        case(
            MapOnly {
                title: "yes".into(),
                extra: BTreeMap::from([("count".into(), 2)]),
            },
            vec![json!({"title":"yes","count":"bad"})],
        ),
        case(
            Tagged::Item {
                child: Child {
                    label: "yes".into(),
                },
                extra: BTreeMap::from([("count".into(), 2)]),
            },
            vec![json!({"kind":"Item","label":"yes","count":false})],
        ),
    ];
    let first = &cases[0]["schema"];
    let reference = first["allOf"][0]["$ref"].as_str().unwrap();
    assert!(
        first
            .pointer(&reference[1..])
            .unwrap()
            .get("unevaluatedProperties")
            .is_some()
    );
    let mut components = Components::new();
    let child = Child::to_schema(&mut components);
    let composed = Generic::<Child>::compose(&mut components, vec![child]);
    cases.push(json!({"schema":{"allOf":[composed],"components":components},"valid":[{"label":"yes","count":2}],"invalid":[{"label":"yes","count":false},{"count":2}]}));
    let ignored = json!(IgnoredMap::compose(&mut Components::new(), vec![]));
    assert!(ignored.get("unevaluatedProperties").is_none());
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
