//! Emitting an OpenAPI 3.2 document.
//!
//! OpenAPI 3.1 stays the default, so 3.2 is opt-in via [`OpenApi::openapi_version`]. Only a 3.2
//! document may carry the `QUERY` operation registered below — under 3.1 the route is skipped
//! with a warning, since OpenAPI 3.1 has no `query` field on a Path Item Object.

use salvo::oapi::extract::*;
use salvo::oapi::{OpenApiVersion, Server as OapiServer, Tag};
use salvo::prelude::*;

#[endpoint(tags("search"))]
async fn hello(name: QueryParam<String, false>) -> String {
    format!("Hello, {}!", name.as_deref().unwrap_or("World"))
}

/// Run a complex search whose description travels in the request content.
#[endpoint(tags("search"))]
async fn search() -> &'static str {
    "results"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("hello").get(hello))
        // `Router::query` registers the HTTP QUERY method, which maps to the OpenAPI 3.2
        // `query` field of the Path Item Object.
        .push(Router::with_path("search").query(search));

    let mut doc = OpenApi::new("3.2 demo api", "0.0.1")
        .openapi_version(OpenApiVersion::Version3_2)
        // `$self` gives the document a stable base URI for relative references (3.2).
        .self_uri("https://example.com/api-doc/openapi.json")
        .merge_router(&router);

    // `name` on a Server Object and `summary`/`kind` on a Tag Object are 3.2 additions.
    doc.servers
        .insert(OapiServer::new("/").name("local").description("local host"));
    doc.tags.insert(
        Tag::new("search")
            .summary("Search")
            .description("Search operations")
            .kind("nav"),
    );

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        // Swagger UI (bundled with salvo-oapi) renders 3.2 documents, including the `query`
        // operation below. Scalar, RapiDoc and ReDoc load the document without complaint but
        // currently ignore `query` operations — see the crate docs for the compatibility notes.
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("/swagger-ui"));

    let acceptor = TcpListener::new("0.0.0.0:8699").bind().await;
    Server::new(acceptor).serve(router).await;
}
