// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
//
// port from https://github.com/seanmonstar/warp/blob/master/examples/websocket_chat.rs

use salvo::extra::ws::{Message, WebSocketUpgrade};
use salvo::http::ParseError;
use salvo::prelude::*;
use salvo::Error;
use salvo_websocket::{handle_socket, WebSocketHandler, WS_CONTROLLER};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Deserialize)]
struct User {
    name: String,
    room: String,
}

#[async_trait]
impl WebSocketHandler for User {
    async fn on_connected(&self, ws_id: usize, sender: UnboundedSender<Result<Message, Error>>) {
        tracing::info!("{} connected", ws_id);
        WS_CONTROLLER
            .write()
            .await
            .join_group(self.room.clone(), sender)
            .unwrap();
        WS_CONTROLLER
            .write()
            .await
            .send_group(self.room.clone(), Message::text(format!("{:?} joined!", self.name)))
            .unwrap();
    }

    async fn on_disconnected(&self, ws_id: usize) {
        tracing::info!("{} disconnected", ws_id);
    }

    async fn on_receive_message(&self, msg: Message) {
        tracing::info!("{:?} received", msg);
        let msg = if let Ok(s) = msg.to_str() {
            s
        } else {
            return;
        };
        let new_msg = format!("<User#{}>: {}", self.name, msg);
        WS_CONTROLLER
            .write()
            .await
            .send_group(self.room.clone(), Message::text(new_msg.clone()))
            .unwrap();
    }

    async fn on_send_message(&self, msg: Message) -> Result<Message, Error> {
        tracing::info!("{:?} sending", msg);
        Ok(msg)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let router = Router::new()
        .handle(index)
        .push(Router::with_path("chat").handle(user_connected));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

#[handler]
async fn user_connected(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    let user: Result<User, ParseError> = req.parse_queries();
    match user {
        Ok(user) => {
            WebSocketUpgrade::new()
                .upgrade(req, res, |ws| async move {
                    handle_socket(ws, user).await;
                })
                .await
        }
        Err(_err) => Err(StatusError::bad_request()),
    }
}

#[handler]
async fn index(res: &mut Response) {
    res.render(Text::Html(INDEX_HTML));
}

static INDEX_HTML: &str = include_str!("./index.html");
