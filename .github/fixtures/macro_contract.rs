use renamed_framework::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, Extractible, renamed_api::ToSchema)]
#[salvo(extract(default_source(from = "query")))]
struct Query {
    value: String,
}

#[derive(renamed_api::ToSchema)]
struct Envelope<T> { value: T }

#[handler]
async fn plain() -> &'static str {
    "ok"
}

#[renamed_api::endpoint]
async fn described() -> &'static str {
    "ok"
}

fn main() {
    let _ = <Query as renamed_framework::extract::Extractible>::metadata();
    let router = Router::with_path("contract").get(described);
    let mut components = renamed_api::Components::new();
    let _ = <Envelope<String> as renamed_api::ToSchema>::to_schema(&mut components);
    let query_schema = <Query as renamed_api::ToSchema>::to_schema(&mut components);
    let doc = renamed_api::OpenApi::new("downstream contracts", "1")
        .merge_router(&router)
        .components(components);
    let mut value = serde_json::to_value(doc).unwrap();
    let parameter = renamed_api::Parameter::new("filter")
        .location(renamed_api::ParameterIn::Query)
        .content(
            "application/json",
            renamed_api::Content::new(query_schema.clone()),
        );
    value["paths"]["/contract"]["get"]["parameters"] = serde_json::json!([parameter]);
    let header = renamed_api::Header::with_content(
        "application/json",
        renamed_api::Content::new(renamed_api::Object::new()),
    );
    value["paths"]["/contract"]["get"]["responses"] = serde_json::json!({"200": {
        "description":"ok", "headers":{"x-contract":header}
    }});
    println!("{}", value);
    let _ = plain;
}
