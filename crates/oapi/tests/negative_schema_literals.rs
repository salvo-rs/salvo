//! Negative schema numbers use Rust literal normalization.
use salvo_oapi::{Components, ToSchema};
use serde_json::json;

#[derive(ToSchema)]
#[allow(dead_code)]
struct Bounds {
    #[salvo(schema(minimum = -1_000, maximum = -0x10))]
    integer: i32,
    #[salvo(schema(minimum = -1_000.25f64, maximum = -10i32))]
    number: f64,
    #[salvo(schema(minimum = 0x10, maximum = 1_000))]
    positive: u32,
}

#[test]
fn separated_radix_and_suffixed_negative_numbers_are_normalized() {
    let mut components = Components::new();
    Bounds::to_schema(&mut components);
    let schemas = json!(components.schemas);
    let schema = schemas.as_object().unwrap().values().next().unwrap();
    assert_eq!(schema["properties"]["integer"]["minimum"], -1000.0);
    assert_eq!(schema["properties"]["integer"]["maximum"], -16.0);
    assert_eq!(schema["properties"]["number"]["minimum"], -1000.25);
    assert_eq!(schema["properties"]["number"]["maximum"], -10.0);
    assert_eq!(schema["properties"]["positive"]["minimum"], 16.0);
    assert_eq!(schema["properties"]["positive"]["maximum"], 1000.0);
}
