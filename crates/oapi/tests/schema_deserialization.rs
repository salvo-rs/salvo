//! Schemas retain their shape when serialized and read back.

use salvo_oapi::schema::{AllOf, AnyOf, BasicType, Object, OneOf};
use salvo_oapi::{Components, Schema, SchemaType};
use serde_json::json;

#[test]
fn typeless_schema_round_trips_in_components() {
    let original = Components::new().add_schema("anything", Object::with_type(SchemaType::any()));
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(json["schemas"]["anything"], json!({}));
    let restored: Components = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), json);
}

#[test]
fn typeless_object_with_constraints_remains_typeless() {
    let input = json!({"description": "accepts every JSON type", "enum": [1, "one", null]});
    let restored: Schema = serde_json::from_value(input.clone()).unwrap();
    assert!(matches!(restored, Schema::Object(_)));
    assert_eq!(serde_json::to_value(restored).unwrap(), input);
}

#[test]
fn typed_compositions_retain_their_schema_variant() {
    let item = || {
        Object::with_type(BasicType::Object).property("name", Object::with_type(BasicType::String))
    };
    for original in [
        Schema::from(OneOf::new().schema_type(BasicType::Object).item(item())),
        Schema::from(AllOf::new().schema_type(BasicType::Object).item(item())),
        Schema::from(AnyOf::new().schema_type(BasicType::Object).item(item())),
    ] {
        let json = serde_json::to_value(&original).unwrap();
        let restored: Schema = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            std::mem::discriminant(&restored),
            std::mem::discriminant(&original),
            "{json}"
        );
        assert_eq!(restored, original);
    }
}

#[test]
fn untyped_compositions_and_arrays_retain_their_schema_variant() {
    for input in [
        json!({"oneOf": [{"type":"string"}, {"type":"integer"}]}),
        json!({"allOf": [{"type":"object"}]}),
        json!({"anyOf": [{"type":"string"}, {"type":"null"}]}),
        json!({"type":"array", "items":{"type":"string"}}),
    ] {
        let restored: Schema = serde_json::from_value(input.clone()).unwrap();
        assert!(!matches!(restored, Schema::Object(_)), "{input}");
        assert_eq!(serde_json::to_value(restored).unwrap(), input);
    }
}

#[test]
fn composition_retains_sibling_array_and_applicator_keywords() {
    let input = json!({
        "type": "array",
        "oneOf": [{"minItems": 1}, {"maxItems": 0}],
        "allOf": [{"uniqueItems": true}],
        "items": {"type": "string"}
    });
    let restored: Schema = serde_json::from_value(input.clone()).unwrap();
    assert!(matches!(restored, Schema::OneOf(_)));
    assert_eq!(serde_json::to_value(restored).unwrap(), input);
}

#[cfg(feature = "yaml")]
#[test]
fn yaml_schemas_preserve_typeless_and_composite_shapes() {
    for input in [
        json!({}),
        json!({"type":"object", "oneOf":[{"type":"object"}]}),
    ] {
        let yaml = serde_norway::to_string(&input).unwrap();
        let restored: Schema = serde_norway::from_str(&yaml).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), input);
    }
}
