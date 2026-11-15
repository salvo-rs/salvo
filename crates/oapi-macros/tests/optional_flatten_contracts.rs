//! Optional flattened fields describe an absent group, not a null parent.
#![allow(dead_code)]
use salvo::oapi::{Components, ComposeSchema, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Serialize, Deserialize, ToSchema)]
struct Child {
    label: String,
    count: u32,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct Parent<T> {
    id: u32,
    #[serde(flatten)]
    child: Option<T>,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct Required {
    id: u32,
    #[serde(flatten)]
    child: Child,
}
fn case<T: Serialize + ToSchema>(values: Vec<T>, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":values,"invalid":invalid})
}
#[test]
fn optional_flatten_keeps_group_constraints() {
    let mut cases = vec![
        case(
            vec![
                Parent::<Child> { id: 1, child: None },
                Parent {
                    id: 1,
                    child: Some(Child {
                        label: "yes".into(),
                        count: 2,
                    }),
                },
            ],
            vec![
                json!({"id":1,"label":false,"count":2}),
                json!({"id":1,"label":"yes"}),
                json!({"id":1,"count":2}),
                json!({"id":"invalid"}),
            ],
        ),
        case(
            vec![Required {
                id: 1,
                child: Child {
                    label: "yes".into(),
                    count: 2,
                },
            }],
            vec![json!({"id":1})],
        ),
    ];
    let mut components = Components::new();
    let child = Child::to_schema(&mut components);
    let composed = json!(Parent::<Child>::compose(&mut components, vec![child]));
    assert!(composed["allOf"][0]["anyOf"].is_array());
    cases.push(json!({"schema":{"allOf":[composed],"components":components},"valid":[{"id":1},{"id":1,"label":"yes","count":2}],"invalid":[{"id":1,"label":false,"count":2},{"id":1,"label":"yes"}]}));
    cases.push(case(
        vec![
            Parent::<std::collections::BTreeMap<String, u32>> { id: 1, child: None },
            Parent {
                id: 1,
                child: Some(std::collections::BTreeMap::from([("extra".into(), 2)])),
            },
        ],
        vec![json!({"id":1,"extra":false})],
    ));
    let _: Parent<Child> = serde_json::from_value(json!({"id":1})).unwrap();
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
