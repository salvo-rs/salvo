use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{tool, tool_router};
use salvo::prelude::*;

#[derive(Clone)]
struct Hello;

#[tool_router(server_handler)]
impl Hello {
    #[tool(description = "Return a greeting")]
    fn hello(&self) -> String {
        "Hello from Salvo!".to_owned()
    }
}

#[tokio::main]
async fn main() {
    let mcp = StreamableHttpService::new(
        || Ok(Hello),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    // The service accepts multiple body types; select Salvo's request body.
    let handler = TowerServiceCompat::<salvo::http::ReqBody, _, _, _>::compat(mcp);
    // A goal forwards every HTTP method used by Streamable HTTP to the SDK.
    let router = Router::with_path("mcp").goal(handler);

    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
