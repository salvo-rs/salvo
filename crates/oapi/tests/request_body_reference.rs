//! Operation request bodies support inline and reusable representations.

use salvo_oapi::extract::{FormFile, FormFiles, JsonBody};
use salvo_oapi::{Components, EndpointArgRegister, OpenApi, Operation, Ref, RefOr, RequestBody};
use serde_json::json;

#[test]
fn request_body_references_survive_document_round_trips() {
    for body in [
        json!({"content":{"application/json":{}}}),
        json!({"$ref":"#/components/requestBodies/Payload"}),
    ] {
        let mut value = serde_json::to_value(OpenApi::new("requests", "1")).unwrap();
        value["components"] =
            json!({"requestBodies":{"Payload":{"content":{"application/json":{}}}}});
        value["paths"] = json!({"/items":{"post":{"requestBody":body,"responses":{"200":{"description":"ok"}}}}});
        let parsed: OpenApi = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&parsed).unwrap()["paths"],
            value["paths"]
        );
        #[cfg(feature = "yaml")]
        {
            let yaml = serde_norway::to_string(&parsed).unwrap();
            let parsed: OpenApi = serde_norway::from_str(&yaml).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap()["paths"],
                value["paths"]
            );
        }
    }
}

#[test]
fn builder_accepts_inline_and_reference_bodies() {
    assert!(matches!(
        Operation::new()
            .request_body(RequestBody::new())
            .request_body,
        Some(RefOr::Type(_))
    ));
    let op = Operation::new().request_body(Ref::new("#/components/requestBodies/Payload"));
    assert_eq!(
        serde_json::to_value(op).unwrap()["requestBody"],
        json!({"$ref":"#/components/requestBodies/Payload"})
    );
}

#[test]
fn inferred_extractors_do_not_replace_explicit_unresolved_references() {
    let mut operation = Operation::new().request_body(Ref::new("https://example.test/body.json"));
    let mut components = Components::new();
    FormFile::register(&mut components, &mut operation, "one");
    FormFiles::register(&mut components, &mut operation, "many");
    JsonBody::<String>::register(&mut components, &mut operation, "json");
    assert_eq!(
        serde_json::to_value(operation).unwrap()["requestBody"],
        json!({"$ref":"https://example.test/body.json"})
    );
}
