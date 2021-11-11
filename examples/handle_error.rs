use salvo::anyhow;
use salvo::prelude::*;

struct CustomError;
#[async_trait]
impl Writer for CustomError {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render_plain_text("custom error");
        res.set_http_error(InternalServerError());
    }
}

#[fn_handler]
async fn handle_anyhow() -> Result<(), anyhow::Error> {
    Err(anyhow::anyhow!("anyhow error"))
}
#[fn_handler]
async fn handle_custom() -> Result<(), CustomError> {
    Err(CustomError)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("anyhow").get(handle_anyhow))
        .push(Router::with_path("custom").get(handle_custom));
    Server::bind(&"127.0.0.1:7878".parse().unwrap())
        .serve(Service::new(router))
        .await
        .unwrap();
}
