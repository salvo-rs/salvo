//! Custom container defaults do not require Default and retain their actual values.
use salvo::oapi::{Components, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(default = "custom_default")]
struct Custom {
    value: String,
    count: u32,
}
fn custom_default() -> Custom {
    Custom {
        value: "custom".into(),
        count: 7,
    }
}

#[derive(Default, Serialize, Deserialize, ToSchema)]
struct Child {
    child: u32,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(default = "Flattened::defaults")]
struct Flattened {
    value: String,
    #[serde(flatten)]
    nested: Child,
}
impl Flattened {
    fn defaults() -> Self {
        Self {
            value: "flattened".into(),
            nested: Child { child: 9 },
        }
    }
}

#[derive(Default, Serialize, Deserialize, ToSchema)]
#[serde(default = "different_default")]
struct Different {
    value: u32,
}
fn different_default() -> Different {
    Different { value: 42 }
}

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let reference = json!(T::to_schema(&mut components));
    json!({"components":components})
        .pointer(&reference["$ref"].as_str().unwrap()[1..])
        .unwrap()
        .clone()
}

#[test]
fn schemas_use_the_same_custom_default_as_serde() {
    let value: Custom = serde_json::from_value(json!({})).unwrap();
    let custom_schema = schema::<Custom>();
    assert_eq!(custom_schema["properties"]["value"]["default"], value.value);
    assert_eq!(custom_schema["properties"]["count"]["default"], value.count);
    let value: Flattened = serde_json::from_value(json!({"child":9})).unwrap();
    assert_eq!(schema::<Flattened>()["default"], json!(value));
    assert_eq!(schema::<Different>()["properties"]["value"]["default"], 42);
}
