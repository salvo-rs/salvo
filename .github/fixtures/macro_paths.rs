// Deliberately avoid the prelude and Writer imports: generated code must resolve
// its own dependencies, including traits used on extractor errors.
#[derive(serde::Deserialize, api::ToSchema)]
struct Payload {
    value: String,
}

#[derive(serde::Deserialize, framework::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Extracted {
    value: String,
}

#[derive(serde::Deserialize, api::ToParameters)]
struct Parameters {
    r#type: String,
}

#[api::endpoint]
async fn parameters(params: Parameters) -> String {
    params.r#type
}

#[derive(api::ToResponse)]
struct Reply(String);

#[derive(api::ToResponses)]
enum Replies {
    #[salvo(response(status_code = 200))]
    Ok(String),
}

#[framework::handler]
async fn plain(value: api::extract::QueryParam<String>) -> String {
    value.into_inner()
}

#[api::endpoint(request_body(content = Payload, description = "payload"))]
async fn described(value: api::extract::QueryParam<String>) -> String {
    value.into_inner()
}

#[api::endpoint(request_body = Payload)]
async fn short() {}

#[api::endpoint(request_body(content = Payload, description = "merged"))]
async fn merged(_body: api::extract::JsonBody<Payload>) {}

#[api::endpoint(request_body(description = "inferred payload"))]
async fn inferred(_body: api::extract::JsonBody<Payload>) {}

#[derive(Clone)]
struct Service;

#[renamed_craft::craft]
impl Service {
    #[craft(endpoint(request_body = Payload))]
    fn endpoint(&self) -> &'static str {
        "ok"
    }

    #[craft(endpoint)]
    fn static_endpoint() {}

    #[craft(handler)]
    fn handler(&self) {}
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    use framework::test::ResponseExt;
    let _ = <Extracted as framework::Extractible>::metadata();
    let router = framework::Router::new()
        .push(framework::Router::with_path("plain").get(plain))
        .push(framework::Router::with_path("described").post(described))
        .push(framework::Router::with_path("short").post(short))
        .push(framework::Router::with_path("merged").post(merged))
        .push(framework::Router::with_path("inferred").post(inferred))
        .push(framework::Router::with_path("craft").post(Service.endpoint()))
        .push(framework::Router::with_path("static").get(Service::static_endpoint()))
        .push(framework::Router::with_path("handler").get(Service.handler()));
    let router = router.push(framework::Router::with_path("parameters").get(parameters));
    let document =
        serde_json::to_value(api::OpenApi::new("paths", "1").merge_router(&router)).unwrap();
    for path in ["/described", "/short", "/merged", "/inferred", "/craft"] {
        assert!(
            document["paths"][path]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"]
                .is_string()
        );
    }
    assert_eq!(
        document["paths"]["/merged"]["post"]["requestBody"]["description"],
        "merged"
    );
    assert_eq!(
        document["paths"]["/inferred"]["post"]["requestBody"]["description"],
        "inferred payload"
    );
    assert!(document["paths"]["/static"]["get"].is_object());
    assert_eq!(
        document["paths"]["/parameters"]["get"]["parameters"][0]["name"],
        "type"
    );
    let service = framework::Service::new(router);
    let response = framework::test::TestClient::get("http://localhost/plain")
        .send(&service)
        .await;
    assert_eq!(
        response.status_code,
        Some(framework::http::StatusCode::BAD_REQUEST)
    );
    let response = framework::test::TestClient::post("http://localhost/described")
        .send(&service)
        .await;
    assert_eq!(
        response.status_code,
        Some(framework::http::StatusCode::BAD_REQUEST)
    );
    let mut response = framework::test::TestClient::get("http://localhost/plain?value=hello")
        .send(&service)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "hello");
    let mut response = framework::test::TestClient::get("http://localhost/parameters?type=hello")
        .send(&service)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "hello");
}
