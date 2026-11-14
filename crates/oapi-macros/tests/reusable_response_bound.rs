//! Reusable endpoint responses require ToResponse without an unrelated ToSchema bound.
#![allow(dead_code)]

use salvo::Router;
use salvo::oapi::{OpenApi, ToResponse, ToSchema, endpoint};
use serde_json::json;

#[derive(ToResponse)]
#[salvo(response(description = "no content"))]
struct NoContent;

#[derive(ToResponse)]
#[salvo(response(description = "a text response"))]
struct TextResponse(String);

#[derive(ToSchema)]
struct Detail {
    active: bool,
}

#[derive(ToResponse)]
struct DetailResponse {
    detail: Detail,
}

#[endpoint(responses((status_code = 204, response = NoContent)))]
async fn remove() {}

#[endpoint(responses((status_code = 200, response = TextResponse)))]
async fn show() {}

#[endpoint(responses((status_code = 200, response = DetailResponse)))]
async fn detail() {}

#[test]
fn response_types_do_not_need_to_schema() {
    let router = Router::new()
        .push(Router::with_path("remove").delete(remove))
        .push(Router::with_path("show").get(show))
        .push(Router::with_path("detail").get(detail));
    let document = json!(OpenApi::new("responses", "1").merge_router(&router));
    let no_content = document["paths"]["/remove"]["delete"]["responses"]["204"]["$ref"]
        .as_str()
        .unwrap();
    assert_eq!(
        document.pointer(&no_content[1..]).unwrap(),
        &json!({ "description": "no content" })
    );

    let text = document["paths"]["/show"]["get"]["responses"]["200"]["$ref"]
        .as_str()
        .unwrap();
    assert_eq!(
        document.pointer(&text[1..]).unwrap()["content"]["text/plain"]["schema"],
        json!({ "type": "string" })
    );
    let response_ref = document["paths"]["/detail"]["get"]["responses"]["200"]["$ref"]
        .as_str()
        .unwrap();
    let response = document.pointer(&response_ref[1..]).unwrap();
    let detail_ref =
        response["content"]["application/json"]["schema"]["properties"]["detail"]["$ref"]
            .as_str()
            .unwrap();
    assert_eq!(
        document.pointer(&detail_ref[1..]).unwrap()["properties"]["active"]["type"],
        "boolean"
    );
}
