//! JSON Schema count and length bounds accept zero.
use salvo_oapi::{Components, ToSchema};
use serde_json::json;

#[derive(ToSchema)]
#[allow(dead_code)]
struct EmptyValues {
    #[salvo(schema(min_length = 0, max_length = 0))]
    text: String,
    #[salvo(schema(min_items = 0, max_items = 0))]
    values: Vec<String>,
}

#[test]
fn zero_bounds_are_preserved_in_schemas() {
    let mut components = Components::new();
    EmptyValues::to_schema(&mut components);
    let schema = json!(components.schemas)
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(schema["properties"]["text"]["minLength"], 0);
    assert_eq!(schema["properties"]["text"]["maxLength"], 0);
    assert_eq!(schema["properties"]["values"]["minItems"], 0);
    assert_eq!(schema["properties"]["values"]["maxItems"], 0);
}
