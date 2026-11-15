//! Internal enum tags are exempt from the payload object's closure and map rules.
#![allow(dead_code)]
use salvo::oapi::schema::{AdditionalProperties, BasicType, Schema};
use salvo::oapi::{Components, ComposeSchema, Object, RefOr, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct Closed {
    value: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
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
struct ComposeOnly;
impl ComposeSchema for ComposeOnly {
    fn compose(_: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        Object::new()
            .property("active", Object::with_type(BasicType::Boolean))
            .required("active")
            .additional_properties(AdditionalProperties::FreeForm(false))
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
    assert!(
        closed["properties"].get("kind").is_none(),
        "must not modify the reusable payload component"
    );
    let mut components = Components::new();
    let generic = ComposeOnly::compose(&mut components, vec![]);
    let composed = Generic::<ComposeOnly>::compose(&mut components, vec![generic]);
    let cases = vec![
        tagged,
        json!({"schema":{"allOf":[composed],"components":components},
        "valid":[{"kind":"Entry","active":true}],
        "invalid":[{"kind":"Entry","active":true,"extra":true},{"kind":"Entry","active":"wrong"}]}),
    ];
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
