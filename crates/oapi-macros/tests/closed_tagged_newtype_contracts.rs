//! Internal enum tags are exempt from the payload object's closure and map rules.
#![allow(dead_code)]
use std::collections::BTreeMap;

use salvo::oapi::schema::{AdditionalProperties, BasicType, Schema};
use salvo::oapi::{Components, ComposeSchema, Object, RefOr, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[salvo(schema(min_properties = 1, max_properties = 1))]
struct Closed {
    value: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[salvo(schema(min_properties = 1, max_properties = 1))]
struct Other {
    count: u32,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum Alternative {
    Text(Closed),
    Number(Other),
}
#[derive(Serialize, Deserialize, ToSchema, Ord, PartialOrd, Eq, PartialEq)]
enum Key {
    #[serde(rename = "x")]
    X,
    #[serde(rename = "y")]
    Y,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum Tagged {
    Entry(Closed),
    Choice(Alternative),
    Map(BTreeMap<Key, u32>),
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Generic<T> {
    Entry(T),
}
#[derive(Serialize, ToSchema)]
#[salvo(schema(min_properties = 2, max_properties = 3))]
struct CountedMap {
    #[serde(flatten)]
    values: BTreeMap<String, u32>,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Counted {
    Entry(CountedMap),
}
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct Nested {
    child: Closed,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum NestedTagged {
    Entry(Nested),
}
struct ComposeOnly;
impl ComposeSchema for ComposeOnly {
    fn compose(_: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        Object::new()
            .property("active", Object::with_type(BasicType::Boolean))
            .required("active")
            .min_properties(1)
            .max_properties(1)
            .additional_properties(AdditionalProperties::FreeForm(false))
            .into()
    }
}
struct CompositeOnly;
impl ComposeSchema for CompositeOnly {
    fn compose(_: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        salvo::oapi::schema::AllOf::new()
            .item(
                Object::new()
                    .property("x", Object::with_type(BasicType::Integer))
                    .property("y", Object::with_type(BasicType::Integer))
                    .additional_properties(AdditionalProperties::FreeForm(false)),
            )
            .add_extension("minProperties", json!(2))
            .add_extension("maxProperties", json!(2))
            .into()
    }
}
fn case<T: Serialize + ToSchema>(valid: Vec<T>, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components}, "valid":valid, "invalid":invalid})
}
#[test]
fn tagged_newtypes_keep_closed_objects_and_map_contracts() {
    let tagged = case(
        vec![
            Tagged::Entry(Closed {
                value: "text".into(),
            }),
            Tagged::Choice(Alternative::Text(Closed {
                value: "text".into(),
            })),
            Tagged::Choice(Alternative::Number(Other { count: 7 })),
            Tagged::Map(BTreeMap::from([(Key::X, 7)])),
        ],
        vec![
            json!({"kind":"Entry","value":"text","extra":true}),
            json!({"kind":"Entry"}),
            json!({"kind":"Other","value":"text"}),
            json!({"kind":"Choice","value":"text","extra":true}),
            json!({"kind":"Choice","value":7}),
            json!({"kind":"Map","z":7}),
            json!({"kind":"Map","x":"wrong"}),
        ],
    );
    assert!(serde_json::from_value::<Tagged>(json!({"kind":"Entry","value":"text"})).is_ok());
    let mut components = Components::new();
    Tagged::to_schema(&mut components);
    let closed_ref = json!(Closed::to_schema(&mut components));
    let document = json!({"components":components});
    let closed = document
        .pointer(&closed_ref["$ref"].as_str().unwrap()[1..])
        .unwrap();
    assert_eq!(closed["additionalProperties"], false);
    assert_eq!(closed["minProperties"], 1);
    assert_eq!(closed["maxProperties"], 1);
    assert!(
        closed["properties"].get("kind").is_none(),
        "must not modify the reusable payload component"
    );
    let mut components = Components::new();
    let generic = ComposeOnly::compose(&mut components, vec![]);
    let composed = Generic::<ComposeOnly>::compose(&mut components, vec![generic]);
    let composite = CompositeOnly::compose(&mut components, vec![]);
    let composite = Generic::<CompositeOnly>::compose(&mut components, vec![composite]);
    let cases = vec![
        tagged,
        case(
            vec![
                Counted::Entry(CountedMap {
                    values: BTreeMap::from([("a".into(), 1), ("b".into(), 2)]),
                }),
                Counted::Entry(CountedMap {
                    values: BTreeMap::from([("a".into(), 1), ("b".into(), 2), ("c".into(), 3)]),
                }),
            ],
            vec![
                json!({"kind":"Entry","a":1}),
                json!({"kind":"Entry","a":1,"b":2,"c":3,"d":4}),
                json!({"kind":"Entry","a":1,"b":"wrong"}),
            ],
        ),
        case(
            vec![NestedTagged::Entry(Nested {
                child: Closed {
                    value: "nested".into(),
                },
            })],
            vec![json!({"kind":"Entry","child":{"value":"nested","extra":true}})],
        ),
        json!({"schema":{"allOf":[composed],"components":components},
        "valid":[{"kind":"Entry","active":true}],
        "invalid":[{"kind":"Entry","active":true,"extra":true},{"kind":"Entry","active":"wrong"}]}),
        json!({"schema":{"allOf":[composite],"components":components},
        "valid":[{"kind":"Entry","x":1,"y":2}],
        "invalid":[{"kind":"Entry","x":1},{"kind":"Entry","x":1,"y":2,"extra":3}]}),
    ];
    let mut components = Components::new();
    let existing_tag = Object::new()
        .property("kind", Object::with_type(BasicType::String))
        .required("kind")
        .min_properties(1)
        .max_properties(1);
    let unchanged =
        salvo::oapi::schema::tagged_payload_schema(existing_tag.into(), "kind", &components);
    assert_eq!(json!(unchanged)["minProperties"], 1);
    assert_eq!(json!(unchanged)["maxProperties"], 1);
    let map_ref = CountedMap::to_schema(&mut components);
    let document = json!({"components":components});
    let map_ref = json!(map_ref);
    let standalone_map = document
        .pointer(&map_ref["$ref"].as_str().unwrap()[1..])
        .unwrap();
    assert_eq!(standalone_map["minProperties"], 2);
    assert_eq!(standalone_map["maxProperties"], 3);
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
