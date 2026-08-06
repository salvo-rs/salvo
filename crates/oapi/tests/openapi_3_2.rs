//! Round-trip fixtures for OpenAPI 3.2 documents.
//!
//! Every 3.2 object added in <https://github.com/salvo-rs/salvo/issues/1685> appears at least
//! once here, so a regression in serialization *or* deserialization shows up as a diff against
//! the fixture rather than as a silently dropped field.

use salvo_oapi::schema::OneOf;
use salvo_oapi::security::{
    ApiKey, ApiKeyValue, DeviceAuthorization, Flow, OAuth2, Scopes, SecurityScheme,
};
use salvo_oapi::{
    BasicType, Components, Content, Discriminator, Encoding, Example, Header, Info, Object,
    OpenApi, OpenApiVersion, Operation, Parameter, ParameterIn, ParameterStyle, PathItem,
    PathItemType, Ref, Response, Schema, Server, Tag, Xml, XmlNodeType,
};
use serde_json::{Value, json};

fn document() -> OpenApi {
    let mut doc = OpenApi::with_info(
        Info::new("streaming api", "1.0.0").summary("A short summary of the API"),
    )
    .openapi_version(OpenApiVersion::Version3_2)
    .self_uri("https://example.com/openapi.json");

    doc.servers.insert(
        Server::new("https://example.com/api")
            .name("production")
            .description("primary host"),
    );

    doc.tags
        .insert(Tag::new("external").summary("External").kind("audience"));
    doc.tags.insert(
        Tag::new("partner")
            .summary("Partner")
            .parent("external")
            .kind("audience"),
    );

    let stream = Content::default()
        .item_schema(Ref::from_schema_name("Frame"))
        .prefix_encoding([Encoding::default().content_type("text/html")])
        .item_encoding(
            Encoding::default()
                .content_type("image/*")
                .encoding("thumb", Encoding::default().content_type("image/png")),
        );

    let response = Response::new("a stream of frames")
        .summary("Frames")
        .add_content("multipart/mixed", stream)
        .add_header(
            "X-Rate-Limit",
            Header::new(Object::with_type(BasicType::Integer)).required(true),
        );

    let search = Operation::new()
        .add_response("200", response)
        .add_parameter(
            Parameter::new("filter")
                .location(ParameterIn::QueryString)
                .content(
                    "application/x-www-form-urlencoded",
                    Content::new(Object::with_type(BasicType::String)),
                ),
        )
        .add_parameter(
            Parameter::new("session")
                .location(ParameterIn::Cookie)
                .style(ParameterStyle::Cookie)
                .schema(Object::with_type(BasicType::String))
                .add_example(
                    "basic",
                    Example::new()
                        .data_value(json!({ "id": 1 }))
                        .serialized_value("session=1"),
                ),
        );

    doc.paths.insert(
        "/search",
        PathItem::new(PathItemType::Query, search)
            .add_additional_operation("PURGE", Operation::new()),
    );

    doc.components = Components::new()
        .add_schema(
            "Frame",
            Schema::from(
                Object::new()
                    .property("kind", Object::with_type(BasicType::String))
                    .xml(Xml::new().node_type(XmlNodeType::Element).name("frame")),
            ),
        )
        .add_schema(
            "AnyFrame",
            Schema::OneOf(
                OneOf::new()
                    .item(Ref::from_schema_name("Frame"))
                    .discriminator(
                        Discriminator::new("kind")
                            .add_mapping("image", "#/components/schemas/Frame")
                            .default_mapping("#/components/schemas/Frame"),
                    ),
            ),
        )
        .add_media_type("FramePayload", Content::new(Ref::from_schema_name("Frame")))
        .add_security_scheme(
            "device",
            SecurityScheme::OAuth2(
                OAuth2::new([Flow::DeviceAuthorization(DeviceAuthorization::new(
                    "https://example.com/device",
                    "https://example.com/token",
                    Scopes::one("read:frames", "read frames"),
                ))])
                .oauth2_metadata_url("https://example.com/.well-known/oauth-authorization-server"),
            ),
        )
        .add_security_scheme(
            "legacy",
            SecurityScheme::ApiKey(ApiKey::Header(
                ApiKeyValue::new("X-Api-Key").deprecated(true),
            )),
        );

    doc
}

/// Every 3.2-only field must reach the wire under its spec name.
#[test]
fn openapi_3_2_document_serializes_all_new_fields() {
    let value: Value = serde_json::to_value(document()).expect("serialize");

    assert_eq!(value["openapi"], json!("3.2.0"));
    assert_eq!(value["$self"], json!("https://example.com/openapi.json"));
    assert_eq!(
        value["info"]["summary"],
        json!("A short summary of the API")
    );

    let server = &value["servers"][0];
    assert_eq!(server["name"], json!("production"));

    let partner = value["tags"]
        .as_array()
        .expect("tags array")
        .iter()
        .find(|tag| tag["name"] == json!("partner"))
        .expect("partner tag");
    assert_eq!(partner["summary"], json!("Partner"));
    assert_eq!(partner["parent"], json!("external"));
    assert_eq!(partner["kind"], json!("audience"));

    let path = &value["paths"]["/search"];
    assert!(path["query"].is_object(), "QUERY operation missing: {path}");
    assert!(path["additionalOperations"]["PURGE"].is_object());

    let params = path["query"]["parameters"].as_array().expect("parameters");
    assert!(params.iter().any(|p| p["in"] == json!("querystring")));
    assert!(params.iter().any(|p| p["style"] == json!("cookie")));

    let example = &params
        .iter()
        .find(|p| p["name"] == json!("session"))
        .expect("session parameter")["examples"]["basic"];
    assert_eq!(example["dataValue"], json!({ "id": 1 }));
    assert_eq!(example["serializedValue"], json!("session=1"));

    let response = &path["query"]["responses"]["200"];
    assert_eq!(response["summary"], json!("Frames"));

    let media = &response["content"]["multipart/mixed"];
    assert!(media["itemSchema"].is_object());
    assert!(media["prefixEncoding"].is_array());
    assert_eq!(media["itemEncoding"]["contentType"], json!("image/*"));
    assert_eq!(
        media["itemEncoding"]["encoding"]["thumb"]["contentType"],
        json!("image/png")
    );

    let components = &value["components"];
    assert!(components["mediaTypes"]["FramePayload"].is_object());

    assert_eq!(
        components["schemas"]["AnyFrame"]["discriminator"]["defaultMapping"],
        json!("#/components/schemas/Frame")
    );
    assert_eq!(
        components["schemas"]["Frame"]["xml"]["nodeType"],
        json!("element")
    );

    let device = &components["securitySchemes"]["device"];
    assert!(device["flows"]["deviceAuthorization"].is_object());
    assert_eq!(
        device["flows"]["deviceAuthorization"]["deviceAuthorizationUrl"],
        json!("https://example.com/device")
    );
    assert_eq!(
        device["oauth2MetadataUrl"],
        json!("https://example.com/.well-known/oauth-authorization-server")
    );
    assert_eq!(
        components["securitySchemes"]["legacy"]["deprecated"],
        json!(true)
    );
}

/// Deserializing our own output must reproduce the same document, and re-serializing it must
/// reproduce the same JSON.
#[test]
fn openapi_3_2_document_round_trips() {
    let doc = document();
    let value = serde_json::to_value(&doc).expect("serialize");

    let parsed: OpenApi = serde_json::from_value(value.clone()).expect("deserialize");
    assert_eq!(parsed, doc);

    let reserialized = serde_json::to_value(&parsed).expect("re-serialize");
    assert_eq!(reserialized, value);
}

/// A 3.1 document must keep its 3.1 shape: none of the new fields appear unless set.
#[test]
fn openapi_3_1_document_is_unaffected() {
    let doc = OpenApi::new("plain api", "1.0.0").add_path(
        "/pets",
        PathItem::new(
            PathItemType::Get,
            Operation::new().add_response("200", Response::new("ok")),
        ),
    );

    let value = serde_json::to_value(&doc).expect("serialize");
    assert_eq!(value["openapi"], json!("3.1.0"));
    assert!(value.get("$self").is_none());
    assert!(
        value["paths"]["/pets"]
            .get("additionalOperations")
            .is_none()
    );
    assert_eq!(
        value["paths"]["/pets"]["get"]["responses"]["200"]["description"],
        json!("ok")
    );

    let parsed: OpenApi = serde_json::from_value(value.clone()).expect("deserialize");
    assert_eq!(serde_json::to_value(&parsed).expect("re-serialize"), value);
}
