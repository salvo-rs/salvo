use futures::{FutureExt, StreamExt};
use tokio;

use salvo::prelude::*;
use salvo_extra::ws::WsHandler;

#[fn_handler]
async fn connect(req: &mut Request, res: &mut Response) -> Result<(), HttpError> {
    match WsHandler::new().handle(req, res){
        Ok(fut) => {
            let fut = async move {
                if let Some(ws) = fut.await {
                    let (tx, rx) = ws.split();
                    let fut = rx.forward(tx).map(|result| {
                        if let Err(e) = result {
                            eprintln!("websocket error: {:?}", e);
                        }
                    });
                    tokio::task::spawn(fut);
                }
            };
            tokio::task::spawn(fut);
            Ok(())
        }
        Err(e) => Err(e)
    }
}

#[tokio::main]
async fn main() {
    let router = Router::new().handle(connect);
    Server::new(router).bind(([127, 0, 0, 1], 7878)).await;
}
