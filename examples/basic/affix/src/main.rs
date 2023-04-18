use std::sync::Arc;
use std::sync::Mutex;

use salvo::affix;
use salvo::prelude::*;

#[handler]
async fn hello(depot: &mut Depot) -> String {
    let config = depot.obtain::<Config>().unwrap();
    let custom_data = depot.get::<&str>("custom_data").unwrap();
    let state = depot.obtain::<Arc<State>>().unwrap();
    let mut fails_ref = state.fails.lock().unwrap();
    fails_ref.push("fail message".into());
    format!("Hello World\nConfig: {config:#?}\nFails: {fails_ref:#?}\nCustom Data: {custom_data}")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(route()).await;
}

#[allow(dead_code)]
#[derive(Default, Clone, Debug)]
struct Config {
    username: String,
    password: String,
}

#[derive(Default, Debug)]
struct State {
    fails: Mutex<Vec<String>>,
}

fn route() -> Router {
    let config = Config {
        username: "root".to_string(),
        password: "pwd".to_string(),
    };
    Router::new()
        .hoop(
            affix::inject(config)
                .inject(Arc::new(State {
                    fails: Mutex::new(Vec::new()),
                }))
                .insert("custom_data", "I love this world!"),
        )
        .get(hello)
        .push(Router::with_path("hello").get(hello))
}
