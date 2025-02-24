use salvo::oapi::extract::*;
use salvo::prelude::*;
use std::sync::Arc;

// Options struct holding a state value for calculations
#[derive(Clone)]
pub struct Opts {
    state: i64,
}

// Implement methods for Opts using the craft macro for API generation
#[craft]
impl Opts {
    // Constructor for Opts
    fn new(state: i64) -> Self {
        Self { state }
    }

    // Handler method that adds state value to two query parameters
    #[craft(handler)]
    fn add1(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.state + *left + *right).to_string()
    }

    // Endpoint method using Arc for shared state
    #[craft(endpoint)]
    pub(crate) fn add2(
        self: ::std::sync::Arc<Self>,
        left: QueryParam<i64>,
        right: QueryParam<i64>,
    ) -> String {
        (self.state + *left + *right).to_string()
    }

    // Static endpoint method with custom error response
    #[craft(endpoint(responses((status_code = 400, description = "Wrong request parameters."))))]
    pub fn add3(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (*left + *right).to_string()
    }
}

#[tokio::main]
async fn main() {
    // Create shared state with initial value 1
    let opts = Arc::new(Opts::new(1));

    // Configure router with three endpoints:
    // - /add1: Uses instance method with state
    // - /add2: Uses Arc-wrapped instance method
    // - /add3: Uses static method without state
    let router = Router::new()
        .push(Router::with_path("add1").get(opts.add1()))
        .push(Router::with_path("add2").get(opts.add2()))
        .push(Router::with_path("add3").get(Opts::add3()));

    // Generate OpenAPI documentation
    let doc = OpenApi::new("Example API", "0.0.1").merge_router(&router);

    // Add OpenAPI documentation and Swagger UI routes
    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));

    // Start server on localhost:5800
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
