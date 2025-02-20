use salvo::catcher::Catcher;
use salvo::prelude::*;

// Handler that returns a simple "Hello World" response
#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

// Handler that deliberately returns a 500 Internal Server Error
#[handler]
async fn error500(res: &mut Response) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Create and start server with custom error handling
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(create_service()).await;
}

// Create service with custom error handling
fn create_service() -> Service {
    // Set up router with two endpoints:
    // - / : Returns "Hello World"
    // - /500 : Triggers a 500 error
    let router = Router::new()
        .get(hello)
        .push(Router::with_path("500").get(error500));

    // Add custom error catcher for 404 Not Found errors
    Service::new(router).catcher(Catcher::default().hoop(handle404))
}

// Custom handler for 404 Not Found errors
#[handler]
async fn handle404(&self, _req: &Request, _depot: &Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    // Check if the error is a 404 Not Found
    if StatusCode::NOT_FOUND == res.status_code.unwrap_or(StatusCode::NOT_FOUND) {
        // Return custom error page
        res.render("Custom 404 Error Page");
        // Skip remaining error handlers
        ctrl.skip_rest();
    }
}
