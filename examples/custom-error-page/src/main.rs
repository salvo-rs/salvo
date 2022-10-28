use salvo::prelude::*;
use salvo::Catcher;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(create_service()).await;
}

fn create_service() -> Service {
    let router = Router::new().get(hello);
    let catchers: Vec<Box<dyn Catcher>> = vec![Box::new(Handle404)];
    Service::new(router).with_catchers(catchers)
}

struct Handle404;
impl Catcher for Handle404 {
    fn catch(&self, _req: &Request, _depot: &Depot, res: &mut Response) -> bool {
        if let Some(StatusCode::NOT_FOUND) = res.status_code() {
            res.render("Custom 404 Error Page");
            true
        } else {
            false
        }
    }
}
