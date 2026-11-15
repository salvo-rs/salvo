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

// Foreign declarations are compile-only: no foreign function is ever called.
// Keep this in the downstream fixture, outside the workspace's unsafe-code ban.
#[framework::handler]
fn foreign_handler() -> String {
    #[allow(dead_code)]
    unsafe extern "C" {
        fn declaration(foreign_handler: u8);
    }
    let _: Self = foreign_handler;
    {
        #[allow(dead_code)]
        unsafe extern "C" {
            fn foreign_handler();
        }
        let foreign_handler = "foreign handler";
        foreign_handler.to_owned()
    }
}

#[api::endpoint]
fn foreign_endpoint() -> String {
    #[allow(dead_code)]
    unsafe extern "C" {
        fn declaration(foreign_endpoint: u8);
    }
    let _: Self = foreign_endpoint;
    {
        #[allow(dead_code)]
        unsafe extern "C" {
            fn foreign_endpoint();
        }
        let foreign_endpoint = "foreign endpoint";
        foreign_endpoint.to_owned()
    }
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

struct GenericService<T, const N: usize>(::std::marker::PhantomData<T>);

#[renamed_craft::craft]
impl<T, const N: usize> GenericService<T, N> {
    const LABEL: &'static str = "generic";
    const WIDTH: usize = N;

    #[craft(handler)]
    fn handler() -> String {
        ::std::format!("{}:{}:{}", Self::LABEL, ::std::any::type_name::<T>(), N)
    }

    #[craft(endpoint(tags(::std::format!("{}-{N}", Self::LABEL))))]
    fn endpoint(value: api::extract::QueryParam<T>) -> framework::writing::Json<[T; N]>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + api::ToSchema + Clone + Send,
        [T; N]: serde::Serialize,
    {
        let value = value.into_inner();
        let values = ::std::array::from_fn(|_| value.clone());
        assert_eq!(values.len(), Self::WIDTH);
        framework::writing::Json(values)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    use framework::test::ResponseExt;
    let _ = <Extracted as framework::Extractible>::metadata();
    let router = framework::Router::new()
        .push(framework::Router::with_path("plain").get(plain))
        .push(framework::Router::with_path("foreign-handler").get(foreign_handler))
        .push(framework::Router::with_path("foreign-endpoint").get(foreign_endpoint))
        .push(framework::Router::with_path("described").post(described))
        .push(framework::Router::with_path("short").post(short))
        .push(framework::Router::with_path("merged").post(merged))
        .push(framework::Router::with_path("inferred").post(inferred))
        .push(framework::Router::with_path("craft").post(Service.endpoint()))
        .push(framework::Router::with_path("static").get(Service::static_endpoint()))
        .push(framework::Router::with_path("handler").get(Service.handler()))
        .push(
            framework::Router::with_path("generic-handler")
                .get(GenericService::<u32, 3>::handler()),
        )
        .push(
            framework::Router::with_path("generic-endpoint")
                .get(GenericService::<u32, 3>::endpoint()),
        );
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
    let generic_operation = &document["paths"]["/generic-endpoint"]["get"];
    assert_eq!(generic_operation["tags"], serde_json::json!(["generic-3"]));
    let generic_schema =
        &generic_operation["responses"]["200"]["content"]["application/json"]["schema"];
    assert_eq!(generic_schema["type"], "array");
    assert_eq!(generic_schema["minItems"], 3);
    assert_eq!(generic_schema["maxItems"], 3);
    assert_eq!(generic_schema["items"]["type"], "integer");
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
    let mut response = framework::test::TestClient::get("http://localhost/generic-handler")
        .send(&service)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "generic:u32:3");
    let mut response =
        framework::test::TestClient::get("http://localhost/generic-endpoint?value=7")
            .send(&service)
            .await;
    assert_eq!(response.take_string().await.unwrap(), "[7,7,7]");
}
