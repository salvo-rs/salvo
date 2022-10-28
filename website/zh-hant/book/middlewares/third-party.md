# 第三方插件

## salvo-websocket

### 添加項目依賴

```toml
[dependencies]
salvo-websocket = "0.0.4"
```

### 定義 ws 連接時的 query params

```rust
#[derive(Debug, Clone, Deserialize)]
struct User {
    name: String,
    room: String,
}
```

### 實現 WebSocketHandler trait

```rust
impl WebSocketHandler for User {
    // 連接事件
    async fn on_connected(&self, ws_id: usize, sender: UnboundedSender<Result<Message, Error>>) {
        tracing::info!("{} connected", ws_id);
        WS_CONTROLLER.write().await.join_group(self.room.clone(), sender).unwrap();
        WS_CONTROLLER.write().await.send_group(
            self.room.clone(),
            Message::text(format!("{:?} joined!", self.name)
            ),
        ).unwrap();
    }

    // 斷連事件
    async fn on_disconnected(&self, ws_id: usize) {
        tracing::info!("{} disconnected", ws_id);
    }

    // 接收消息事件
    async fn on_receive_message(&self, msg: Message) {
        tracing::info!("{:?} received", msg);
        let msg = if let Ok(s) = msg.to_str() {
            s
        } else {
            return;
        };
        let new_msg = format!("<User#{}>: {}", self.name, msg);
        WS_CONTROLLER.write().await.send_group(self.room.clone(), Message::text(new_msg.clone())).unwrap();
    }

    async fn on_send_message(&self, msg: Message) -> Result<Message, Error> {
        tracing::info!("{:?} sending", msg);
        Ok(msg)
    }
}
```

### 編寫連接處理方法

```rust
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let router = Router::new()
        .push(Router::with_path("chat").handle(user_connected));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

#[handler]
async fn user_connected(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    let user: Result<User, ParseError> = req.parse_queries();
    match user {
        Ok(user) => {
            WebSocketUpgrade::new().upgrade(req, res, |ws| async move {
                handle_socket(ws, user).await;
            }).await
        }
        Err(_err) => {
            Err(StatusError::bad_request())
        }
    }
}
```

更多內容，請直接查閱[示例](https://github.com/salvo-rs/salvo/tree/main/examples/ws-chat-with-salvo-websocket)
