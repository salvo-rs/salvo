use salvo::prelude::*;

// Custom error type for demonstration
struct CustomError;

// Implement Writer trait for CustomError to customize error response
#[async_trait]
impl Writer for CustomError {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        // Set response status code to 500 and custom error message
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render("custom error");
    }
}

// Handler that returns an anyhow error for testing error handling
#[handler]
async fn handle_anyhow() -> Result<(), anyhow::Error> {
    Err(anyhow::anyhow!("handled anyhow error"))
}

// Handler that returns an eyre error for testing error handling
#[handler]
async fn handle_eyre() -> eyre::Result<()> {
    Err(eyre::Report::msg("handled eyre error"))
}

// Handler that returns our custom error type
#[handler]
async fn handle_custom() -> Result<(), CustomError> {
    Err(CustomError)
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Set up router with three error handling endpoints:
    // - /anyhow : demonstrates anyhow error handling
    // - /eyre : demonstrates eyre error handling
    // - /custom : demonstrates custom error handling
    let router = Router::new()
        .push(Router::with_path("anyhow").get(handle_anyhow))
        .push(Router::with_path("eyre").get(handle_eyre))
        .push(Router::with_path("custom").get(handle_custom));

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
