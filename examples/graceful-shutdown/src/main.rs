use salvo_core::prelude::*;
use tokio::signal;

#[tokio::main]
async fn main() {
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    let server = Server::new(acceptor);
    let handle = server.handle();

    // Listen Shutdown Signal
    listen_shutdown_signal(handle);

    server.serve(Router::new()).await;
}

async fn listen_shutdown_signal(handle: ServerHandle) {
    // Wait Shutdown Signal
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        // Graceful Shutdown Server
        handle.stop_graceful(None);
    })
}
