//! OpenAPI 3.1 documents may describe components or webhooks without paths.

use salvo_oapi::OpenApi;
use serde_json::{Value, json};

fn documents() -> [Value; 2] {
    [
        json!({
            "openapi": "3.1.0", "info": {"title": "Shared types", "version": "1"},
            "components": {"schemas": {"Pet": {"type": "object", "properties": {"name": {"type": "string"}}}}}
        }),
        json!({
            "openapi": "3.1.0", "info": {"title": "Notifications", "version": "1"},
            "webhooks": {"petChanged": {"post": {"responses": {"200": {"description": "Accepted"}}}}}
        }),
    ]
}

#[test]
fn component_and_webhook_documents_without_paths_can_be_imported() {
    for version in ["3.1.0", "3.2.0"] {
        for mut input in documents() {
            input["openapi"] = json!(version);
            let document: OpenApi = serde_json::from_value(input.clone()).unwrap();
            assert!(document.paths.is_empty());
            let output = serde_json::to_value(&document).unwrap();
            for key in ["components", "webhooks"] {
                assert_eq!(output.get(key), input.get(key));
            }
            assert_eq!(output["paths"], json!({}));
        }
    }
}

#[cfg(feature = "yaml")]
#[test]
fn optional_paths_work_in_yaml_documents() {
    for input in documents() {
        let yaml = serde_norway::to_string(&input).unwrap();
        let document: OpenApi = serde_norway::from_str(&yaml).unwrap();
        assert!(document.paths.is_empty());
        let round_trip: OpenApi = serde_norway::from_str(&document.to_yaml().unwrap()).unwrap();
        assert_eq!(round_trip, document);
    }
}

#[test]
fn explicit_paths_are_still_preserved() {
    let input = json!({
        "openapi": "3.1.0", "info": {"title": "Pets", "version": "1"},
        "paths": {"/pets": {"get": {"responses": {"200": {"description": "OK"}}}}}
    });
    let document: OpenApi = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(document).unwrap()["paths"],
        input["paths"]
    );
}

#[test]
fn required_document_fields_and_invalid_paths_remain_errors() {
    for input in [
        json!({"openapi": "3.1.0", "paths": {}}),
        json!({"info": {"title": "Pets", "version": "1"}, "paths": {}}),
        json!({"openapi": "3.1.0", "info": {"title": "Pets", "version": "1"}, "paths": "invalid"}),
    ] {
        assert!(serde_json::from_value::<OpenApi>(input).is_err());
    }
}
