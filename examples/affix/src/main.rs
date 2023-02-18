use salvo::affix;
use salvo::prelude::*;

#[handler]
async fn hello(depot: &mut Depot) -> String {
    let config = depot.obtain::<Config>().unwrap();
    let custom_data = depot.get::<&str>("custom_data").unwrap();
    format!("Hello World\nConfig: {config:#?}\nCustom Data: {custom_data}")
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

fn route() -> Router {
    let config = Config {
        username: "root".to_string(),
        password: "pwd".to_string(),
    };
    Router::new()
        .hoop(affix::inject(config).insert("custom_data", "I love this world!"))
        .get(hello)
        .push(Router::with_path("hello").get(hello))
}
