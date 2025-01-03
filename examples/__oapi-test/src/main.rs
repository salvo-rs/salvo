use salvo::oapi::extract::*;
use salvo::prelude::*;
use serde::*;

#[derive(Debug, Serialize, Deserialize, ToResponses)]
pub enum GeneralError {
    #[salvo(response(status_code = 400, description = "Bad Request"))]
    BadRequest(String),
    #[salvo(response(status_code = 429, description = "Rate Limit Exceeded"))]
    RateLimit { message: String, retry_after: u64 },
    #[salvo(response(status_code = 500, description = "Something went wrong"))]
    InternalServerError(String),
}

#[endpoint]
async fn hello(name: QueryParam<String, false>) -> String {
    format!("Hello, {}!", name.as_deref().unwrap_or("World"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().push(Router::with_path("hello").get(hello));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("/swagger-ui"));

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
