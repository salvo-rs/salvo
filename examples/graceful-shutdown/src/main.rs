use salvo::prelude::*;
use salvo::server::ServerHandle;
use tokio::signal;

#[tokio::main]
async fn main() {
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    let server = Server::new(acceptor);
    let handle = server.handle();

    // Listen Shutdown Signal
    tokio::spawn(listen_shutdown_signal(handle));

    server.serve(Router::new()).await;
}

async fn listen_shutdown_signal(handle: ServerHandle) {
    // Wait Shutdown Signal
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(windows)]
    let terminate = async {
        signal::windows::ctrl_c()
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => println!("ctrl_c signal received"),
        _ = terminate => println!("terminate signal received"),
    };

    // Graceful Shutdown Server
    handle.stop_graceful(None);
}
