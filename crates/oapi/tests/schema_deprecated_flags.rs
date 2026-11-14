//! Schema annotations describe the property and honor explicit false values.
#![allow(dead_code)]

use salvo_oapi::{Components, RefOr, ToSchema};
use serde_json::{Value, json};

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let RefOr::Ref(reference) = T::to_schema(&mut components) else {
        panic!("expected named schema");
    };
    json!({ "components": components })
        .pointer(&reference.ref_location[1..])
        .unwrap()
        .clone()
}

#[test]
#[allow(deprecated)]
fn explicit_deprecation_flags_keep_their_values() {
    #[derive(ToSchema)]
    struct Model {
        #[salvo(schema(deprecated = false))]
        current: String,
        #[salvo(schema(deprecated = true))]
        old: String,
        #[salvo(schema(deprecated))]
        shorthand: String,
        plain: String,
        #[deprecated]
        #[salvo(schema(deprecated = false))]
        rust_deprecated: String,
    }
    let value = schema::<Model>();
    assert_eq!(value["properties"]["current"]["deprecated"], false);
    assert_eq!(value["properties"]["old"]["deprecated"], true);
    assert_eq!(value["properties"]["shorthand"]["deprecated"], true);
    assert_ne!(value["properties"]["plain"]["deprecated"], true);
    assert_eq!(value["properties"]["rust_deprecated"]["deprecated"], true);
}
