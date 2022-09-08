use futures_util::{FutureExt, StreamExt};

use salvo::extra::ws::{WebSocket, WebSocketUpgrade};
use salvo::prelude::*;

#[handler]
async fn connect(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    WebSocketUpgrade::new().handle(req, res, handle_socket).await
}
async fn handle_socket(mut ws: WebSocket) {
    while let Some(msg) = ws.recv().await {
        let msg = if let Ok(msg) = msg {
            msg
        } else {
            // client disconnected
            return;
        };

        if socket.send(msg).await.is_err() {
            // client disconnected
            return;
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let router = Router::new().handle(connect);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
