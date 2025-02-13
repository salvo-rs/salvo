use salvo::prelude::*;
use salvo::server::ServerHandle;
use tokio::signal;

#[tokio::main]
async fn main() {
    // Bind server to port 5800
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    // Create server instance
    let server = Server::new(acceptor);
    // Get server handle for graceful shutdown
    let handle = server.handle();

    // Listen Shutdown Signal
    tokio::spawn(listen_shutdown_signal(handle));

    // Start serving requests (empty router in this example)
    server.serve(Router::new()).await;
}

async fn listen_shutdown_signal(handle: ServerHandle) {
    // Wait Shutdown Signal
    let ctrl_c = async {
        // Handle Ctrl+C signal
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        // Handle SIGTERM on Unix systems
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(windows)]
    let terminate = async {
        // Handle Ctrl+C on Windows (alternative implementation)
        signal::windows::ctrl_c()
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    // Wait for either signal to be received
    tokio::select! {
        _ = ctrl_c => println!("ctrl_c signal received"),
        _ = terminate => println!("terminate signal received"),
    };

    // Graceful Shutdown Server
    handle.stop_graceful(None);
}
