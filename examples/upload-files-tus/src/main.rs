use salvo::prelude::*;
use salvo::tus::{Tus, options::MaxSize};
use salvo::oapi::extract::*;

#[endpoint(tags("main"), summary = "hello", description = "description  of the  main endpoint")]
async fn hello(name: QueryParam<String, false>, res: &mut Response){
    println!("{:?}", name);
    res.render(format!("Hello, {}!", name.clone().unwrap_or("Unknown".into())))
}

#[endpoint(tags("Hello"), summary="Just Print hello world", description= "description of the handle/endpoint to print hello world")]
async fn hello_world() -> Result<&'static str, salvo::Error>{
    Ok("Hello world")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world)
        .push(Router::with_path("hello").get(hello));

    // let tus = Tus::new().path("/files")
    //     .relative_location(true)
    //     .max_size(MaxSize::Fixed(10 * 1024 * 1024))
    //     .with_upload_id_naming_function(|_req, _metadata| {
    //         // Here you can implement your own logic to generate unique IDs for uploads.
    //         // For simplicity, we'll use a fixed ID in this example.
    //         Ok("unique-upload-id-12345".to_string())
    //     });

    let tus = Tus::new().path("/files")
        .relative_location(true)
        .max_size(MaxSize::Fixed(10 * 1024 * 1024));
    let router = tus.into_router();

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    Server::new(acceptor).serve(router).await;
}