use salvo::cors::Cors;
use salvo::http::Method;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();
    // Start both backend and frontend servers concurrently
    tokio::join!(backend_server(), frontend_server());
}

async fn backend_server() {
    // Handler that returns a simple message for CORS demonstration
    #[handler]
    async fn hello() -> &'static str {
        "hello, I am content from remote server."
    }

    // Configure CORS middleware with specific settings:
    // - Allow requests from localhost:5800
    // - Allow specific HTTP methods
    // - Allow authorization header
    let cors = Cors::new()
        .allow_origin(["http://127.0.0.1:5800", "http://localhost:5800"])
        .allow_methods(vec![Method::GET, Method::POST, Method::DELETE])
        .allow_headers("authorization")
        .into_handler();

    // Set up backend router with CORS protection
    let router = Router::with_path("hello").post(hello);
    let service = Service::new(router).hoop(cors);

    // Start backend server on port 5600
    let acceptor = TcpListener::new("0.0.0.0:5600").bind().await;
    Server::new(acceptor).serve(service).await;
}

async fn frontend_server() {
    // Handler that serves the HTML page with CORS test
    #[handler]
    async fn index() -> Text<&'static str> {
        Text::Html(HTML_DATA)
    }

    // Set up frontend router to serve the test page
    let router = Router::new().get(index);
    // Start frontend server on port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

// HTML template with JavaScript code to test CORS
// Contains a button that triggers a POST request to the backend server
const HTML_DATA: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width">
    <title>Salvo Cors</title>
</head>
<body>
<button id="btn">Load Content</button>
<div id="content"></div>
<script>
document.getElementById("btn").addEventListener("click", function() {
    fetch("http://127.0.0.1:5600/hello", {method: "POST", headers: {authorization: "abcdef"}}).then(function(response) {
        return response.text();
    }).then(function(data) {
        document.getElementById("content").innerHTML = data;
    });
});
</script>
</body>
</html>
"#;
