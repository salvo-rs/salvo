//! Adjacent variant titles remain valid Rust and annotate the outer alternative.
#![allow(dead_code)]
use salvo::oapi::{Components, ToSchema};
use serde_json::{Value, json};

#[derive(serde::Serialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum Message {
    #[salvo(schema(title = "Text payload", rename = "text"))]
    Text(String),
    #[salvo(schema(title = "Record payload"))]
    Record {
        active: bool,
    },
    Untitled(u32),
}

#[test]
fn adjacent_newtype_and_named_titles_describe_the_outer_variant() {
    let mut components = Components::new();
    let reference = json!(Message::to_schema(&mut components));
    let document = json!({"components": components});
    let schema = document
        .pointer(&reference["$ref"].as_str().unwrap()[1..])
        .unwrap();
    let text = &schema["oneOf"][0];
    assert_eq!(text["title"], "Text payload");
    assert_eq!(text["properties"]["kind"]["enum"], json!(["text"]));
    assert_eq!(text["properties"]["data"]["type"], "string");
    assert_eq!(schema["oneOf"][1]["title"], "Record payload");
    assert_eq!(
        schema["oneOf"][1]["properties"]["data"]["properties"]["active"]["type"],
        "boolean"
    );
    assert!(schema["oneOf"][2].get("title").is_none());
    let value: Value = serde_json::to_value(Message::Record { active: true }).unwrap();
    assert_eq!(value, json!({"kind": "Record", "data": {"active": true}}));
}
