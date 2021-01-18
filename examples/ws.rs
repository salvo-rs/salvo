use salvo::prelude::*;

use salvo_extra::ws::{WsHandler, WebSocket};

#[tokio::main]
async fn main() {
    let handler = WsHandler::new(|_, _| {
        |socket: WebSocket| {
            // Just echo all messages back...
            let (tx, rx) = socket.split();
            rx.forward(tx).map(|result| {
                if let Err(e) = result {
                    eprintln!("websocket error: {:?}", e);
                }
            })
        }
    });
    let router = Router::new().handle(handler);
    Server::new(router).run(([127, 0, 0, 1], 7878)).await;
}
