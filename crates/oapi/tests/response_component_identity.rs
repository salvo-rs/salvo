//! Reusable responses with different Rust types need distinct component identities.
#![allow(dead_code)]

use salvo_core::Router;
use salvo_oapi::{OpenApi, ToResponse, ToSchema, endpoint};
use serde_json::json;

mod text {
    #[derive(salvo_oapi::ToResponse)]
    pub(super) struct Reply(String);
}

mod number {
    #[derive(salvo_oapi::ToResponse)]
    pub(super) struct Reply(i32);
}

#[endpoint(responses((status_code = 200, response = text::Reply)))]
async fn text_reply() {}

#[endpoint(responses((status_code = 200, response = number::Reply)))]
async fn number_reply() {}

#[test]
fn same_named_types_in_different_modules_do_not_overwrite_responses() {
    let router = Router::new()
        .push(Router::with_path("text").get(text_reply))
        .push(Router::with_path("number").get(number_reply));
    let doc = json!(OpenApi::new("responses", "1").merge_router(&router));
    assert_valid_responses(&doc);
    assert_ne!(
        doc["paths"]["/text"]["get"]["responses"]["200"]["$ref"],
        doc["paths"]["/number"]["get"]["responses"]["200"]["$ref"]
    );
}

#[derive(ToResponse)]
struct GenericReply<T: ToSchema>(T);

#[endpoint(responses((status_code = 200, response = GenericReply<String>)))]
async fn generic_text() {}

#[endpoint(responses((status_code = 200, response = GenericReply<i32>)))]
async fn generic_number() {}

#[test]
fn generic_instantiations_do_not_overwrite_responses() {
    let router = Router::new()
        .push(Router::with_path("text").get(generic_text))
        .push(Router::with_path("number").get(generic_number));
    let doc = json!(OpenApi::new("responses", "1").merge_router(&router));
    assert_valid_responses(&doc);
    assert_ne!(
        doc["paths"]["/text"]["get"]["responses"]["200"]["$ref"],
        doc["paths"]["/number"]["get"]["responses"]["200"]["$ref"]
    );
}

#[derive(ToResponse)]
struct BorrowedReply<'a>(&'a str);

fn borrowed_response<'a>(_: &'a str) {
    let mut components = salvo_oapi::Components::new();
    let _ = BorrowedReply::<'a>::to_response(&mut components);
}

fn assert_valid_responses(document: &serde_json::Value) {
    for key in document["components"]["responses"]
        .as_object()
        .unwrap()
        .keys()
    {
        assert!(!key.is_empty());
        assert!(
            key.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
            "invalid component key: {key}"
        );
    }
    for path in ["/text", "/number"] {
        let reference = document["paths"][path]["get"]["responses"]["200"]["$ref"]
            .as_str()
            .unwrap();
        assert!(document.pointer(&reference[1..]).is_some());
    }
}

#[derive(ToResponse)]
#[allow(non_camel_case_types)]
struct GenericReply_3Cu32_3E;

#[derive(ToResponse)]
struct Réponse;

#[test]
fn encoded_keys_preserve_literal_escape_sequences_and_unicode() {
    let mut components = salvo_oapi::Components::new();
    let generic = GenericReply::<u32>::to_response(&mut components);
    let literal = GenericReply_3Cu32_3E::to_response(&mut components);
    let unicode = Réponse::to_response(&mut components);
    assert_ne!(json!(generic), json!(literal));
    let document = json!({"components": components});
    for response in [generic, literal, unicode] {
        let reference = json!(response)["$ref"].as_str().unwrap().to_owned();
        let key = reference.strip_prefix("#/components/responses/").unwrap();
        assert!(
            key.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        );
        assert!(document.pointer(&reference[1..]).is_some());
    }
}
