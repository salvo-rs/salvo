use futures_util::{FutureExt, StreamExt};

use salvo::extra::ws::WsHandler;
use salvo::prelude::*;

#[fn_handler]
async fn connect(req: &mut Request, res: &mut Response) -> Result<(), HttpError> {
    let fut = WsHandler::new().handle(req, res)?;
    let fut = async move {
        if let Some(ws) = fut.await {
            let (tx, rx) = ws.split();
            let fut = rx.forward(tx).map(|result| {
                if let Err(e) = result {
                    tracing::error!(error = ?e, "websocket error");
                }
            });
            tokio::task::spawn(fut);
        }
    };
    tokio::task::spawn(fut);
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let router = Router::new().handle(connect);
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(router).await;
}
