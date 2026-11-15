//! Container field rules apply to every enum representation and respect overrides.
#![allow(dead_code)]
use salvo::oapi::{Components, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

macro_rules! case {
    ($name:ident $(, $attr:meta)?) => {
        #[derive(Serialize, Deserialize, ToSchema)]
        #[serde(rename_all_fields(serialize = "camelCase", deserialize = "snake_case"))]
        $(#[$attr])?
        enum $name {
            Default { first_name: String },
            #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
            Override { last_name: String },
            Explicit { #[serde(rename = "chosen")] field_name: String },
        }
    };
}
case!(External);
case!(Internal, serde(tag = "kind"));
case!(Adjacent, serde(tag = "kind", content = "data"));
case!(Untagged, serde(untagged));

fn required_property(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(fields) => {
            (fields
                .get("properties")
                .is_some_and(|props| props.get(name).is_some())
                && fields
                    .get("required")
                    .is_some_and(|required| required.as_array().unwrap().contains(&json!(name))))
                || fields.values().any(|child| required_property(child, name))
        }
        Value::Array(values) => values.iter().any(|child| required_property(child, name)),
        _ => false,
    }
}
fn check<T: ToSchema>() {
    let mut components = Components::new();
    T::to_schema(&mut components);
    let schemas = json!(components);
    for name in ["firstName", "LAST_NAME", "chosen"] {
        assert!(required_property(&schemas, name), "{schemas}");
    }
    assert!(!required_property(&schemas, "first_name"));
}

#[test]
fn enum_field_rules_match_serde_in_all_representations() {
    check::<External>();
    check::<Internal>();
    check::<Adjacent>();
    check::<Untagged>();
    assert_eq!(
        json!(External::Default {
            first_name: "a".into()
        }),
        json!({"Default":{"firstName":"a"}})
    );
    assert_eq!(
        json!(Internal::Default {
            first_name: "a".into()
        }),
        json!({"kind":"Default","firstName":"a"})
    );
    assert_eq!(
        json!(Adjacent::Default {
            first_name: "a".into()
        }),
        json!({"kind":"Default","data":{"firstName":"a"}})
    );
    assert_eq!(
        json!(Untagged::Default {
            first_name: "a".into()
        }),
        json!({"firstName":"a"})
    );
    assert!(serde_json::from_value::<External>(json!({"Default":{"first_name":"a"}})).is_ok());
}
