use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;

use salvo_core::routing::Router;
use salvo_core::Server;
use salvo_extra::serve::Static;

#[tokio::main]
async fn main() {
    let listener = UnixListener::bind("/tmp/salvo.sock").unwrap();
    let incoming = UnixListenerStream::new(listener);
    let router = Router::new().path("files/<*path>").get(Static::from("./static"));
    Server::new(router).run_incoming(incoming)
        .await;
}
