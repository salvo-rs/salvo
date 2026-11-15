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
#[derive(Serialize, Deserialize, ToSchema)]
struct StringParent<T> {
    id: String,
    #[serde(flatten)]
    child: Option<T>,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct ChildMap {
    label: String,
    #[serde(flatten)]
    extra: std::collections::BTreeMap<String, u32>,
}
#[derive(Serialize, Deserialize, ToSchema, PartialEq, Eq, PartialOrd, Ord)]
enum MapKey {
    Allowed,
}
fn case<T: Serialize + serde::de::DeserializeOwned + ToSchema>(
    values: Vec<T>,
    invalid: Vec<Value>,
) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    let valid = serde_json::to_value(values).unwrap();
    for value in valid.as_array().unwrap() {
        let _: T = serde_json::from_value(value.clone()).unwrap();
    }
    json!({"schema":{"allOf":[schema],"components":components},"valid":valid,"invalid":invalid})
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
    cases.push(case(
        vec![
            StringParent::<std::collections::BTreeMap<String, u32>> {
                id: "one".into(),
                child: None,
            },
            StringParent {
                id: "one".into(),
                child: Some(std::collections::BTreeMap::from([("extra".into(), 2)])),
            },
        ],
        vec![json!({"id":2}), json!({"id":"one","extra":false})],
    ));
    cases.push(case(
        vec![
            StringParent::<ChildMap> {
                id: "one".into(),
                child: None,
            },
            StringParent {
                id: "one".into(),
                child: Some(ChildMap {
                    label: "yes".into(),
                    extra: std::collections::BTreeMap::from([("extra".into(), 2)]),
                }),
            },
        ],
        vec![
            json!({"id":"one","label":false}),
            json!({"id":"one","label":"yes","extra":false}),
        ],
    ));
    cases.push(case(
        vec![
            StringParent::<std::collections::BTreeMap<MapKey, u32>> {
                id: "one".into(),
                child: None,
            },
            StringParent {
                id: "one".into(),
                child: Some(std::collections::BTreeMap::from([(MapKey::Allowed, 2)])),
            },
        ],
        vec![json!({"id":"one","Invalid":2})],
    ));
    for inline in [false, true] {
        let mut components = Components::new();
        let child = if inline {
            ChildMap::compose(&mut components, vec![])
        } else {
            ChildMap::to_schema(&mut components)
        };
        let original = components.schemas.clone();
        let composed = StringParent::<ChildMap>::compose(&mut components, vec![child]);
        for (key, schema) in original {
            assert_eq!(
                components.schemas.get(&key),
                Some(&schema),
                "shared component changed: {key}"
            );
        }
        cases.push(json!({"schema":{"allOf":[composed],"components":components},"valid":[{"id":"one"},{"id":"one","label":"yes","extra":2}],"invalid":[{"id":2},{"id":"one","label":"yes","extra":false}]}));
    }
    let parent = salvo::oapi::Object::new()
        .property(
            "id",
            salvo::oapi::Object::with_type(salvo::oapi::schema::BasicType::String),
        )
        .required("id");
    let child = salvo::oapi::Object::new()
        .property(
            "label",
            salvo::oapi::Object::with_type(salvo::oapi::schema::BasicType::String),
        )
        .required("label")
        .add_extension("unevaluatedProperties", json!(false));
    let child =
        salvo::oapi::schema::optional_flatten_schema(child.into(), &parent, &Components::new());
    assert_eq!(
        json!(&child)["anyOf"][0]["properties"]["label"]["type"],
        "string"
    );
    cases.push(json!({"schema":{"allOf":[child,parent]},"valid":[{"id":"one"},{"id":"one","label":"yes"}],"invalid":[{"id":"one","label":2},{"id":"one","label":"yes","unknown":2}]}));

    let mut components = Components::new();
    let mut child = ChildMap::to_schema(&mut components);
    if let salvo::oapi::RefOr::Ref(reference) = &mut child {
        reference
            .extensions
            .insert("maxProperties".into(), json!(3));
        reference.description = "child map annotation".into();
        reference.summary = "child map summary".into();
    }
    let composed = StringParent::<ChildMap>::compose(&mut components, vec![child]);
    let serialized = serde_json::to_string(&composed).unwrap();
    assert!(serialized.contains("child map annotation"));
    assert!(serialized.contains("child map summary"));
    cases.push(json!({"schema":{"allOf":[composed],"components":components},"valid":[{"id":"one"},{"id":"one","label":"yes","extra":2}],"invalid":[{"id":"one","label":"yes","extra":2,"another":3},{"id":"one","label":2}]}));
    let _: Parent<Child> = serde_json::from_value(json!({"id":1})).unwrap();
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
