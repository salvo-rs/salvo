//! Catch-all map constraints apply after all flattened object properties.
#![allow(dead_code)]
use std::collections::BTreeMap;

use salvo::oapi::{Components, ComposeSchema, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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

#[derive(Serialize, Deserialize, ToSchema)]
struct GenericParent<T> {
    id: String,
    #[serde(flatten)]
    child: T,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct ChildMap {
    label: String,
    #[serde(flatten)]
    values: BTreeMap<String, u32>,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct NestedMap {
    #[serde(flatten)]
    child: Child,
    #[serde(flatten)]
    values: BTreeMap<String, u32>,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct Fixed {
    enabled: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct OptionalWithSibling<T> {
    id: String,
    #[serde(flatten)]
    fixed: Fixed,
    #[serde(flatten)]
    child: Option<T>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum Conditional {
    A { a: String },
    B { b: String },
}

#[derive(Serialize, Deserialize, ToSchema)]
struct ConditionalMap<T> {
    #[serde(flatten)]
    variant: Conditional,
    #[serde(flatten)]
    values: T,
}

fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}

fn checked_case<T: Serialize + serde::de::DeserializeOwned + ToSchema>(
    value: T,
    invalid: Vec<Value>,
) -> Value {
    let encoded = serde_json::to_value(&value).unwrap();
    let decoded: T = serde_json::from_value(encoded.clone())
        .unwrap_or_else(|error| panic!("{}: {error}; value={encoded}", std::any::type_name::<T>()));
    assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
    case(value, invalid)
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

    cases.push(checked_case(
        GenericParent {
            id: "one".into(),
            child: BTreeMap::from([("extra".to_owned(), 2_u32)]),
        },
        vec![json!({"id":"one","extra":false}), json!({"id":2,"extra":2})],
    ));
    cases.push(checked_case(
        GenericParent {
            id: "one".into(),
            child: ChildMap {
                label: "yes".into(),
                values: BTreeMap::from([("extra".into(), 2)]),
            },
        },
        vec![
            json!({"id":"one","label":false,"extra":2}),
            json!({"id":"one","label":"yes","extra":"bad"}),
        ],
    ));
    cases.push(checked_case(
        GenericParent {
            id: "one".into(),
            child: NestedMap {
                child: Child {
                    label: "yes".into(),
                },
                values: BTreeMap::from([("extra".into(), 2)]),
            },
        },
        vec![
            json!({"id":"one","label":"yes","extra":"bad"}),
            json!({"id":"one","extra":2}),
        ],
    ));
    for child in [None, Some(BTreeMap::from([("extra".to_owned(), 2_u32)]))] {
        cases.push(checked_case(
            OptionalWithSibling {
                id: "one".into(),
                fixed: Fixed { enabled: true },
                child,
            },
            vec![
                json!({"id":"one","enabled":true,"extra":"bad"}),
                json!({"id":"one","enabled":"bad"}),
            ],
        ));
    }
    for child in [
        None,
        Some(ChildMap {
            label: "yes".into(),
            values: BTreeMap::from([("extra".into(), 2)]),
        }),
    ] {
        cases.push(checked_case(
            OptionalWithSibling {
                id: "one".into(),
                fixed: Fixed { enabled: true },
                child,
            },
            vec![
                json!({"id":"one","enabled":true,"label":"yes","extra":"bad"}),
                json!({"id":"one","enabled":false,"label":false}),
            ],
        ));
    }
    // Only the active variant's fields are evaluated. A field belonging to
    // another variant must still satisfy the generic catch-all's value type.
    // This checks a real serialized output; serde's flattened tagged-enum
    // deserializer does not claim its fields before the subsequent map read.
    cases.push(case(
        ConditionalMap {
            variant: Conditional::A { a: "yes".into() },
            values: BTreeMap::from([("b".to_owned(), 2_u32)]),
        },
        vec![
            json!({"kind":"A","a":"yes","b":"bad"}),
            json!({"kind":"B","b":"yes","a":"bad"}),
        ],
    ));
    let mut components = Components::new();
    let child = ChildMap::to_schema(&mut components);
    let original_components = components.schemas.clone();
    let composed = GenericParent::<ChildMap>::compose(&mut components, vec![child.clone()]);
    assert_eq!(components.schemas, original_components);
    cases.push(json!({"schema":{"allOf":[composed],"components":components.clone()},"valid":[{"id":"one","label":"yes","extra":2}],"invalid":[{"id":"one","label":"yes","extra":"bad"}]}));
    cases.push(json!({"schema":{"allOf":[child],"components":components},"valid":[{"label":"yes","extra":2}],"invalid":[{"label":"yes","id":"one"}]}));
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
