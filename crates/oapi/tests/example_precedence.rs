//! Named examples consistently take precedence over a singular example.
#![allow(unused_braces)]
use salvo_core::Router;
use salvo_oapi::{Content, Example, OpenApi, Parameter, ParameterIn, endpoint};
use serde_json::{Value, json};

fn named() -> [(String, Example); 1] {
    [("many".into(), Example::new().value(json!("many")))]
}
fn parameter() -> Parameter {
    Parameter::new("q").location(ParameterIn::Query)
}
fn assert_named(value: Value) {
    assert!(value.get("example").is_none(), "{value}");
    assert_eq!(value["examples"]["many"]["value"], "many");
}

#[test]
fn builders_keep_named_examples_in_both_orders() {
    assert_named(json!(
        Content::default()
            .example(json!("single"))
            .extend_examples(named())
    ));
    assert_named(json!(
        Content::default()
            .extend_examples(named())
            .example(json!("single"))
    ));
    assert_named(json!(
        parameter().example(json!("single")).examples(named())
    ));
    assert_named(json!(
        parameter().examples(named()).example(json!("single"))
    ));
    assert_named(json!(
        parameter()
            .example(json!("single"))
            .add_example("many", Example::new().value(json!("many")))
    ));
}

#[test]
fn empty_named_lists_do_not_remove_the_singular_example() {
    let empty = || std::iter::empty::<(String, Example)>();
    assert_eq!(
        json!(
            Content::default()
                .example(json!("single"))
                .extend_examples(empty())
        )["example"],
        "single"
    );
    assert_eq!(
        json!(parameter().example(json!("single")).examples(empty()))["example"],
        "single"
    );
}

#[test]
fn merging_parameters_preserves_named_precedence_and_empty_updates() {
    let mut value = parameter().example(json!("single"));
    assert!(value.merge(parameter().examples(named())));
    assert_named(json!(&value));
    assert!(value.merge(parameter().example(json!("later"))));
    assert_named(json!(value));
    let mut value = parameter().example(json!("single"));
    assert!(value.merge(parameter().examples(std::iter::empty::<(String, Example)>())));
    assert_eq!(json!(value)["example"], "single");
}

#[test]
fn public_fields_and_deserialized_values_serialize_with_the_same_precedence() {
    let mut content = Content::default().extend_examples(named());
    content.example = Some(json!("single"));
    assert_named(json!(content));
    let mut parameter = parameter().example(json!("single"));
    parameter
        .examples
        .insert("many".into(), Example::new().value(json!("many")).into());
    assert_named(json!(parameter));
    let content: Content =
        serde_json::from_value(json!({"example":"single", "examples":{"many":{"value":"many"}}}))
            .unwrap();
    assert_named(json!(content));
    let parameter: Parameter = serde_json::from_value(
        json!({"name":"q", "in":"query", "example":"single", "examples":{"many":{"value":"many"}}}),
    )
    .unwrap();
    assert_named(json!(parameter));
}

#[endpoint(responses((status_code = 200, body = String, example = json!("single"), examples(("many" = (value = json!("many")))))))]
fn response() -> &'static str {
    "ok"
}
#[endpoint(request_body(content = String, example = json!("single"), examples(("many" = (value = json!("many"))))))]
fn request() {}

#[test]
fn endpoint_request_and_response_macros_follow_builder_precedence() {
    let response_doc =
        json!(OpenApi::new("examples", "1").merge_router(&Router::new().get(response)));
    assert_named(
        response_doc["paths"]["/"]["get"]["responses"]["200"]["content"]["text/plain"].clone(),
    );
    let request_doc =
        json!(OpenApi::new("examples", "1").merge_router(&Router::new().post(request)));
    assert_named(request_doc["paths"]["/"]["post"]["requestBody"]["content"]["text/plain"].clone());
}

#[endpoint(responses((status_code = 200, body = String, example = json!("single"), examples())))]
fn empty_response() {}
#[endpoint(request_body(content = String, example = json!("single"), examples()))]
fn empty_request() {}

#[test]
fn empty_macro_example_lists_keep_the_singular_example() {
    let response_doc =
        json!(OpenApi::new("examples", "1").merge_router(&Router::new().get(empty_response)));
    assert_eq!(
        response_doc["paths"]["/"]["get"]["responses"]["200"]["content"]["text/plain"]["example"],
        "single"
    );
    let request_doc =
        json!(OpenApi::new("examples", "1").merge_router(&Router::new().post(empty_request)));
    assert_eq!(
        request_doc["paths"]["/"]["post"]["requestBody"]["content"]["text/plain"]["example"],
        "single"
    );
}
