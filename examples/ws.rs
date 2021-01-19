use futures::{FutureExt, StreamExt};
use tokio;

use salvo::prelude::*;
use salvo_extra::ws::{WebSocket, WsHandler};

fn callback(_req: Request, _depot: Depot, ws: WebSocket) {
    // Just echo all messages back...
    let (tx, rx) = ws.split();
    let fut = rx.forward(tx).map(|result| {
        if let Err(e) = result {
            eprintln!("websocket error: {:?}", e);
        }
    });
    tokio::task::spawn(fut);
}

#[tokio::main]
async fn main() {
    let router = Router::new().handle(WsHandler::new(callback));
    Server::new(router).run(([127, 0, 0, 1], 7878)).await;
}
