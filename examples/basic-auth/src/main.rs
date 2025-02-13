use salvo::basic_auth::{BasicAuth, BasicAuthValidator};
use salvo::prelude::*;

// Custom validator implementing BasicAuthValidator trait
struct Validator;
impl BasicAuthValidator for Validator {
    // Validate username and password combination
    async fn validate(&self, username: &str, password: &str, _depot: &mut Depot) -> bool {
        username == "root" && password == "pwd"
    }
}

// Simple handler that returns "Hello" for authenticated requests
#[handler]
async fn hello() -> &'static str {
    "Hello"
}

// Create router with basic authentication middleware
fn route() -> Router {
    // Initialize basic authentication handler with our validator
    let auth_handler = BasicAuth::new(Validator);
    // Apply authentication middleware to the router
    Router::with_hoop(auth_handler).goal(hello)
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt().init();

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(route()).await;
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_basic_auth() {
        // Create a service instance from our router for testing purposes
        let service = Service::new(super::route());

        // Test case 1: Verify successful authentication with valid credentials
        let content = TestClient::get("http://0.0.0.0:5800/")
            .basic_auth("root", Some("pwd")) // Use correct username/password
            .send(&service) // Send the request to the service
            .await
            .take_string() // Extract response body as string
            .await
            .unwrap();
        // Verify response contains expected "Hello" message
        assert!(content.contains("Hello"));

        // Test case 2: Verify authentication failure with invalid password
        let content = TestClient::get("http://0.0.0.0:5800/")
            .basic_auth("root", Some("pwd2")) // Use incorrect password
            .send(&service) // Send the request to the service
            .await
            .take_string() // Extract response body as string
            .await
            .unwrap();
        // Verify response contains "Unauthorized" error
        assert!(content.contains("Unauthorized"));
    }
}
