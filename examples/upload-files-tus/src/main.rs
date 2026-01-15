use nanoid::nanoid;
use salvo::oapi::extract::*;
use salvo::prelude::*;
use salvo::tus::Tus;
use salvo::tus::options::MaxSize;
use tracing::info;

#[endpoint(
    tags("main"),
    summary = "hello",
    description = "description  of the  main endpoint"
)]
async fn hello(name: QueryParam<String, false>, res: &mut Response) {
    println!("{:?}", name);
    res.render(format!(
        "Hello, {}!",
        name.clone().unwrap_or("Unknown".into())
    ))
}

#[endpoint(
    tags("Hello"),
    summary = "Just Print hello world",
    description = "description of the handle/endpoint to print hello world"
)]
async fn hello_world() -> Result<&'static str, salvo::Error> {
    Ok("Hello world")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let tus = Tus::new().path("/files")
        .relative_location(true)
        .max_size(MaxSize::Fixed(2 * 1024 * 1024 * 1024)) // 2GBi
        .with_on_incoming_request(|_req, id| async move {
            info!("On Incoming Request: {}", id);
        })
        .with_upload_id_naming_function(|_req, _metadata| async {
            // Use nanoid
            let id = nanoid!();
            Ok(id)
        })
        .with_on_incoming_request_sync(|_req, id| {
            info!("Current File ID: {}", id);
        });

    let router = Router::new()
        .get(hello_world)
        .push(Router::with_path("hello").get(hello))
        .push(tus.into_router());

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    Server::new(acceptor).serve(router).await;
}
