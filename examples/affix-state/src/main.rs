use std::sync::Arc;
use std::sync::Mutex;

use salvo::prelude::*;

// Configuration structure with username and password
#[allow(dead_code)]
#[derive(Default, Clone, Debug)]
struct Config {
    username: String,
    password: String,
}

// State structure to hold a list of fail messages
#[derive(Default, Debug)]
struct State {
    fails: Mutex<Vec<String>>,
}

#[handler]
async fn hello(depot: &mut Depot) -> String {
    // Obtain the Config instance from the depot
    let config = depot.obtain::<Config>().unwrap();
    // Get custom data from the depot
    let custom_data = depot.get::<&str>("custom_data").unwrap();
    // Obtain the shared State instance from the depot
    let state = depot.obtain::<Arc<State>>().unwrap();
    // Lock the fails vector and add a new fail message
    let mut fails_ref = state.fails.lock().unwrap();
    fails_ref.push("fail message".into());
    // Format and return the response string
    format!("Hello World\nConfig: {config:#?}\nFails: {fails_ref:#?}\nCustom Data: {custom_data}")
}

#[tokio::main]
async fn main() {
    // Initialize the tracing subscriber for logging
    tracing_subscriber::fmt().init();

    // Create a Config instance with default username and password
    let config = Config {
        username: "root".to_string(),
        password: "pwd".to_string(),
    };

    // Set up the router with state injection and custom data
    let router = Router::new()
        // Use hoop to inject middleware and data into the request context
        .hoop(
            affix_state::inject(config)
                // Inject a shared State instance into the request context
                .inject(Arc::new(State {
                    fails: Mutex::new(Vec::new()),
                }))
                // Insert custom data into the request context
                .insert("custom_data", "I love this world!"),
        )
        // Register the hello handler for the root path
        .get(hello)
        // Add an additional route for the path "/hello" with the same handler
        .push(Router::with_path("hello").get(hello));

    // Bind the server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
