use rust_embed::RustEmbed;
use salvo::prelude::*;
use salvo::serve_static::EmbeddedFileExt;

#[derive(RustEmbed)]
#[folder = "static"]
struct Assets;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(serve_file);
    
    Server::new(TcpListener::bind("127.0.0.1:7878").await).serve(router).await;
}

#[handler]
async fn serve_file(req: &mut Request, res: &mut Response) {
    let path = req.param::<String>("**path").unwrap();
    if let Some(file) = Assets::get(&path) {
        file.render(req, res);
    }
}
