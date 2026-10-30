//! Security Scheme and Reference Objects must coexist in reusable components.

use salvo_oapi::security::{
    ApiKey, ApiKeyValue, Flow, Http, HttpAuthScheme, OAuth2, OpenIdConnect,
};
use salvo_oapi::{Components, OpenApi, Ref, RefOr, SecurityScheme};
use serde_json::{Value, json};

fn security_schemes() -> Value {
    json!({
        "Base": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"},
        "Local": {
            "$ref": "#/components/securitySchemes/Base",
            "summary": "Local authentication",
            "description": "Authentication for the local API"
        },
        "External": {
            "$ref": "https://example.invalid/security.yaml#/components/securitySchemes/Shared",
            "description": "Shared authentication"
        }
    })
}

#[test]
fn components_security_schemes_preserve_reference_objects() {
    let value = json!({"securitySchemes": security_schemes()});
    let components: Components = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(components).unwrap(), value);
}

#[test]
fn openapi_security_scheme_references_roundtrip_json() {
    for version in ["3.1.0", "3.2.0"] {
        let value = json!({
            "openapi": version,
            "info": {"title": "Reference contract", "version": "1"},
            "paths": {},
            "components": {"securitySchemes": security_schemes()}
        });
        let doc: OpenApi = serde_json::from_value(value).unwrap();
        let encoded = serde_json::to_value(&doc).unwrap();
        assert_eq!(encoded["components"]["securitySchemes"], security_schemes());
        assert_eq!(serde_json::from_value::<OpenApi>(encoded).unwrap(), doc);
    }
}

#[cfg(feature = "yaml")]
#[test]
fn components_security_scheme_references_roundtrip_yaml() {
    let value = json!({"securitySchemes": security_schemes()});
    let yaml = serde_norway::to_string(&value).unwrap();
    let components: Components = serde_norway::from_str(&yaml).unwrap();
    let encoded = serde_norway::to_string(&components).unwrap();
    assert_eq!(serde_norway::from_str::<Value>(&encoded).unwrap(), value);

    for version in ["3.1.0", "3.2.0"] {
        let value = json!({
            "openapi": version,
            "info": {"title": "Reference contract", "version": "1"},
            "paths": {},
            "components": {"securitySchemes": security_schemes()}
        });
        let yaml = serde_norway::to_string(&value).unwrap();
        let doc: OpenApi = serde_norway::from_str(&yaml).unwrap();
        let encoded = serde_norway::to_string(&doc).unwrap();
        let value: Value = serde_norway::from_str(&encoded).unwrap();
        assert_eq!(value["components"]["securitySchemes"], security_schemes());
        assert_eq!(serde_norway::from_str::<OpenApi>(&encoded).unwrap(), doc);
    }
}

#[test]
fn security_scheme_builders_accept_references_and_existing_inline_inputs() {
    let components = Components::new()
        .add_security_scheme(
            "Http",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        )
        .add_security_scheme("ApiKey", ApiKey::Header(ApiKeyValue::new("api-key")))
        .add_security_scheme("OAuth2", OAuth2::new(std::iter::empty::<Flow>()))
        .add_security_scheme(
            "OpenId",
            OpenIdConnect::new("https://example.invalid/discovery"),
        )
        .add_security_scheme(
            "Local",
            Ref::new("#/components/securitySchemes/Http").description("Local scheme"),
        )
        .extend_security_schemes([("External", Ref::new("security.yaml#/Shared"))]);
    let doc = OpenApi::new("Reference contract", "1")
        .add_security_scheme(
            "Http",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        )
        .add_security_scheme("ApiKey", ApiKey::Header(ApiKeyValue::new("api-key")))
        .add_security_scheme("OAuth2", OAuth2::new(std::iter::empty::<Flow>()))
        .add_security_scheme(
            "OpenId",
            OpenIdConnect::new("https://example.invalid/discovery"),
        )
        .add_security_scheme(
            "Local",
            Ref::new("#/components/securitySchemes/Http").description("Local scheme"),
        )
        .extend_security_schemes([("External", Ref::new("security.yaml#/Shared"))]);
    assert_eq!(doc.components, components);
    let value = serde_json::to_value(&components).unwrap();
    assert_eq!(
        value["securitySchemes"]["Http"],
        json!({"type":"http", "scheme":"bearer"})
    );
    assert_eq!(
        value["securitySchemes"]["Local"],
        json!({"$ref":"#/components/securitySchemes/Http", "description":"Local scheme"})
    );
    assert_eq!(
        value["securitySchemes"]["External"],
        json!({"$ref":"security.yaml#/Shared"})
    );

    // The iterator builders still accept the inline input types directly.
    assert_eq!(
        Components::new()
            .extend_security_schemes([("ApiKey", ApiKey::Header(ApiKeyValue::new("api-key")))]),
        OpenApi::new("Reference contract", "1")
            .extend_security_schemes([("ApiKey", ApiKey::Header(ApiKeyValue::new("api-key")))])
            .components,
    );
    assert!(matches!(
        components.security_schemes["Http"],
        RefOr::Type(_)
    ));
    assert!(matches!(
        components.security_schemes["Local"],
        RefOr::Ref(_)
    ));
}

#[test]
fn appending_security_scheme_references_preserves_the_existing_entry() {
    let local = Ref::new("#/components/securitySchemes/Http").summary("Keep this reference");
    let mut components = Components::new().add_security_scheme("Local", local.clone());
    let mut other = Components::new()
        .add_security_scheme(
            "Local",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Basic)),
        )
        .add_security_scheme("External", Ref::new("security.yaml#/Shared"));
    components.append(&mut other);
    assert_eq!(components.security_schemes["Local"], RefOr::Ref(local));
    assert_eq!(
        serde_json::to_value(&components).unwrap()["securitySchemes"]["External"],
        json!({"$ref":"security.yaml#/Shared"})
    );
}
