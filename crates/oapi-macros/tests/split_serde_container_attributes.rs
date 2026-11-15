//! Split container attributes retain the same enum representation in either order.
use salvo::oapi::{Components, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
enum Combined {
    Entry { value: String },
    Empty,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
#[serde(content = "data")]
enum TagFirst {
    Entry { value: String },
    Empty,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(content = "data")]
#[serde(tag = "kind")]
enum ContentFirst {
    Entry { value: String },
    Empty,
}

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let reference = json!(T::to_schema(&mut components));
    json!({"components": components})
        .pointer(&reference["$ref"].as_str().unwrap()[1..])
        .unwrap()
        .clone()
}

#[test]
fn split_attributes_match_combined_schema_and_serde_payloads() {
    assert_eq!(schema::<TagFirst>(), schema::<Combined>());
    assert_eq!(schema::<ContentFirst>(), schema::<Combined>());
    let value = json!({"kind":"Entry", "data":{"value":"hello"}});
    assert_eq!(
        json!(TagFirst::Entry {
            value: "hello".into()
        }),
        value
    );
    assert_eq!(
        json!(ContentFirst::Entry {
            value: "hello".into()
        }),
        value
    );
    assert!(serde_json::from_value::<TagFirst>(value.clone()).is_ok());
    assert!(serde_json::from_value::<ContentFirst>(value).is_ok());
    assert_eq!(json!(TagFirst::Empty), json!({"kind":"Empty"}));
}
