//! Object schemas retain the JSON Schema title keyword independently of names.
use salvo_oapi::Object;
use serde_json::json;

#[test]
fn object_title_round_trips_without_replacing_name() {
    let object = Object::new().name("component").title("Readable title");
    let value = json!(object);
    assert_eq!(value["name"], "component");
    assert_eq!(value["title"], "Readable title");
    assert_eq!(serde_json::from_value::<Object>(value).unwrap(), object);
    assert!(json!(Object::new()).get("title").is_none());
}
