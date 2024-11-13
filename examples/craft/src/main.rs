use salvo::oapi::extract::*;
use salvo::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct Opts {
    state: i64,
}

#[craft]
impl Opts {
    fn new(state: i64) -> Self {
        Self { state }
    }
    /// doc line 1
    /// doc line 2
    #[craft(handler)]
    fn add1(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.state + *left + *right).to_string()
    }
    /// doc line 3
    /// doc line 4
    #[craft(endpoint)]
    pub(crate) fn add2(
        self: ::std::sync::Arc<Self>,
        left: QueryParam<i64>,
        right: QueryParam<i64>,
    ) -> String {
        (self.state + *left + *right).to_string()
    }
    /// doc line 5
    /// doc line 6
    #[craft(endpoint(responses((status_code = 400, description = "Wrong request parameters."))))]
    pub fn add3(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (*left + *right).to_string()
    }
}

#[tokio::main]
async fn main() {
    let opts = Arc::new(Opts::new(1));
    let router = Router::new()
        .push(Router::with_path("add1").get(opts.add1()))
        .push(Router::with_path("add2").get(opts.add2()))
        .push(Router::with_path("add3").get(Opts::add3()));
    let doc = OpenApi::new("Example API", "0.0.1").merge_router(&router);
    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
