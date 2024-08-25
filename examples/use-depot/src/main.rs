use salvo::prelude::*;

#[handler]
async fn set_user(depot: &mut Depot) {
    depot.insert("user", "client");
}
#[handler]
async fn hello(depot: &mut Depot) -> String {
    format!(
        "Hello {}",
        depot.get::<&str>("user").copied().unwrap_or_default()
    )
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().hoop(set_user).goal(hello);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
