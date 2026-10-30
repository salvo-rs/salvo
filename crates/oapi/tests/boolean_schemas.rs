//! Boolean schemas are valid schemas in the OpenAPI 3.1 and 3.2 dialects.
use salvo_oapi::{Components, OpenApi, RefOr, Schema};
use serde_json::json;

#[test]
fn boolean_schemas_round_trip_at_root_and_nested_positions() {
    for input in [
        json!(true),
        json!(false),
        json!({"type":"object", "properties":{"allowed":true,"forbidden":false},"propertyNames":true,"additionalProperties":false}),
        json!({"allOf":[true, {"type":"string"}]}),
        json!({"anyOf":[false, {"type":"string"}]}),
        json!({"oneOf":[false, {"type":"integer"}]}),
        json!({"type":"array", "items":true,"prefixItems":[true,false]}),
        json!({"type":"array", "items":false,"prefixItems":[true,false]}),
    ] {
        let schema: Schema = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("{input}: {error}"));
        assert_eq!(serde_json::to_value(schema).unwrap(), input);
    }
    for input in [json!(true), json!(false)] {
        let schema: RefOr<Schema> = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(serde_json::to_value(schema).unwrap(), input);
    }
}

#[test]
fn components_and_operation_content_accept_boolean_schemas() {
    for version in ["3.1.0", "3.2.0"] {
        let input = json!({
            "openapi":version,"info":{"title":"Boolean schemas","version":"1"},
            "components":{"schemas":{"Anything":true,"Nothing":false}},
            "paths":{"/validate":{"post":{
                "parameters":[{"name":"value","in":"query","schema":true}],
                "requestBody":{"content":{"application/json":{"schema":false}}},
                "responses":{"200":{"description":"OK","content":{"application/json":{"schema":true}}}}
            }}}
        });
        let document: OpenApi = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(serde_json::to_value(document).unwrap(), input);
    }
    let input = json!({"schemas":{"Anything":true,"Nothing":false}});
    let components: Components = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(components).unwrap(), input);
}

#[cfg(feature = "yaml")]
#[test]
fn yaml_boolean_schemas_round_trip() {
    let input = json!({"schemas":{"Anything":true,"Nothing":false,"Envelope":{"type":"object","properties":{"reserved":false}}}});
    let yaml = serde_norway::to_string(&input).unwrap();
    let components: Components = serde_norway::from_str(&yaml).unwrap();
    assert_eq!(serde_json::to_value(components).unwrap(), input);
}

#[test]
fn builders_support_boolean_schemas_and_preserve_existing_boolean_forms() {
    use salvo_oapi::schema::AdditionalProperties;
    use salvo_oapi::{Array, ArrayItems, Object};
    let components = Components::new()
        .add_schema("Anything", true)
        .add_schema("Nothing", Schema::from(false))
        .add_schema("Envelope", Object::new().property("reserved", false))
        .add_schema(
            "Tuple",
            Array::new()
                .items(ArrayItems::False)
                .prefix_items([true, false]),
        );
    let value = serde_json::to_value(&components).unwrap();
    assert_eq!(value["schemas"]["Anything"], json!(true));
    assert_eq!(value["schemas"]["Nothing"], json!(false));
    assert_eq!(
        value["schemas"]["Envelope"]["properties"]["reserved"],
        json!(false)
    );
    let restored: Components = serde_json::from_value(value).unwrap();
    assert_eq!(restored, components);
    for allowed in [true, false] {
        let form: AdditionalProperties<Schema> = serde_json::from_value(json!(allowed)).unwrap();
        assert_eq!(form, AdditionalProperties::FreeForm(allowed));
    }
}
