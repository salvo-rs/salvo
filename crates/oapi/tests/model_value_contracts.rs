//! JSON Schema and OpenAPI values retain their wire representation.
use salvo_oapi::extract::{FormBody, JsonBody};
use salvo_oapi::{
    Components, ComposeSchema, Content, Example, ExternalDocs, Header, Link, Number, OpenApi,
    Parameter, Paths, Schema, ToRequestBody, ToSchema, Xml,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn round_trip<T: serde::de::DeserializeOwned + Serialize>(value: Value) {
    let decoded: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn explicit_null_and_omitted_json_annotations_are_distinct() {
    for value in [Value::Null, json!(17), json!({"name":"item"})] {
        round_trip::<Example>(json!({"value":value}));
        round_trip::<Example>(json!({"dataValue":value}));
        round_trip::<Link>(json!({"requestBody":value}));
        round_trip::<Content>(json!({"example":value}));
        round_trip::<Header>(json!({"example":value}));
        round_trip::<Parameter>(json!({"name":"id","in":"query","example":value}));
        for schema in [
            json!({"type":["string","null"],"default":value}),
            json!({"type":"array","items":true,"default":value}),
            json!({"oneOf":[{"type":"null"}],"default":value}),
            json!({"allOf":[{"type":"null"}],"default":value}),
            json!({"anyOf":[{"type":"null"}],"default":value}),
        ] {
            round_trip::<Schema>(schema);
        }
    }
    round_trip::<Example>(json!({}));
    round_trip::<Schema>(json!({"type":"string"}));
}

#[test]
fn url_only_external_docs_omit_description() {
    assert_eq!(
        serde_json::to_value(ExternalDocs::new("https://example.com/docs")).unwrap(),
        json!({"url":"https://example.com/docs"})
    );
}

#[test]
fn extensions_round_trip_all_json_shapes() {
    for value in [
        Value::Null,
        json!(true),
        json!(7),
        json!("text"),
        json!([1, 2]),
        json!({"a":1}),
    ] {
        round_trip::<Example>(json!({"value":1,"x-extra":value}));
        round_trip::<Xml>(json!({"name":"thing","x-extra":value}));
        round_trip::<Link>(json!({"operationId":"read","x-extra":value}));
        round_trip::<Paths>(json!({"/items":{"get":{"responses":{}}},"x-extra":value}));
    }
}

#[test]
fn paths_extensions_survive_merge_and_nesting() {
    let source: OpenApi = serde_json::from_value(json!({
        "openapi":"3.1.0","info":{"title":"source","version":"1"},
        "paths":{"/items":{"get":{"responses":{}}},"x-extra":true}
    }))
    .unwrap();
    let merged = OpenApi::new("test", "1").merge(source.clone());
    assert_eq!(
        serde_json::to_value(merged).unwrap()["paths"]["x-extra"],
        true
    );
    let nested = OpenApi::new("test", "1").nest("/api", source);
    let result = serde_json::to_value(nested).unwrap();
    assert_eq!(result["paths"]["x-extra"], true);
    assert!(result["paths"].get("/api/items").is_some());
}

#[test]
fn numbers_keep_fixed_width_integer_boundaries() {
    for n in [u64::MAX, 4_294_967_296, 9_007_199_254_740_993] {
        let value: Number = n.into();
        assert!(matches!(value, Number::UInt(v) if v==n));
        round_trip::<Number>(json!(n));
    }
    for n in [i64::MIN, -4_294_967_296] {
        round_trip::<Number>(json!(n));
    }
}

#[test]
fn unit_schema_accepts_only_null() {
    let mut components = Components::new();
    assert_eq!(
        serde_json::to_value(<()>::to_schema(&mut components)).unwrap(),
        json!({"type":"null"})
    );
    assert_eq!(
        serde_json::to_value(<()>::compose(&mut components, vec![])).unwrap(),
        json!({"type":"null"})
    );
}

#[derive(Deserialize, ToSchema)]
#[allow(dead_code)]
struct Body {
    name: String,
}

#[test]
fn body_extractors_document_required_request_bodies() {
    let mut components = Components::new();
    for body in [
        JsonBody::<Body>::to_request_body(&mut components),
        FormBody::<Body>::to_request_body(&mut components),
    ] {
        assert_eq!(serde_json::to_value(body).unwrap()["required"], true);
    }
}

#[cfg(feature = "chrono")]
#[test]
fn chrono_duration_matches_serde_tuple_for_direct_and_derived_schemas() {
    use chrono::Duration;
    #[derive(Serialize, ToSchema)]
    struct Durations {
        bare: Duration,
        qualified: chrono::Duration,
    }
    let value = Durations {
        bare: Duration::seconds(3),
        qualified: Duration::seconds(3),
    };
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({"bare":[3,0],"qualified":[3,0]})
    );
    let mut components = Components::new();
    let direct = serde_json::to_value(Duration::to_schema(&mut components)).unwrap();
    assert_eq!(direct["type"], "array");
    assert_eq!(direct["minItems"], 2);
    assert_eq!(direct["maxItems"], 2);
    assert_eq!(direct["prefixItems"][0]["type"], "integer");
    let reference = serde_json::to_value(Durations::to_schema(&mut components)).unwrap();
    let doc = json!({"components":components});
    let schema = doc
        .pointer(&reference["$ref"].as_str().unwrap()[1..])
        .unwrap();
    for name in ["bare", "qualified"] {
        assert_eq!(schema["properties"][name], direct);
    }
}
