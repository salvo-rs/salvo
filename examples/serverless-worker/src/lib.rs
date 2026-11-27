//! Cloudflare Workers demo built on Salvo's serverless runtime.
//!
//! This example shows the full surface of the Workers adapter:
//!
//! - plain-text and JSON responses
//! - path and query parameter extraction
//! - JSON request body parsing
//! - Workers bindings access (`KV`, `D1`, `R2`, `Queue`, `Var`, `Secret`, `Context`)
//! - WebSocket echo with close-frame handling
//!
//! Build and deploy it with:
//!
//! ```text
//! cargo build --release --target wasm32-unknown-unknown
//! wrangler deploy
//! ```
//!
//! The binding names used below match the placeholders in [`wrangler.toml`](../wrangler.toml).
//! Replace the placeholder ids with your own resources before deploying.

use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use salvo::fs::StreamFile;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use worker::Range;
use worker::send::{IntoSendFuture, SendWrapper};

/// JSON payload echoed back by `POST /echo`.
#[derive(Debug, Deserialize, Serialize)]
struct EchoPayload {
    /// Message to echo.
    message: String,
}

/// Value stored under a KV key via `PUT /kv/<key>`.
#[derive(Debug, Deserialize)]
struct KvPutPayload {
    /// Value to persist.
    value: String,
}

/// Message pushed to the queue via `POST /queue`.
#[derive(Debug, Deserialize, Serialize)]
struct QueueMessage {
    /// Text carried by the message.
    text: String,
}

/// A row read from the D1 `users` table.
#[derive(Debug, Deserialize, Serialize)]
struct User {
    /// Primary key.
    id: i64,
    /// Display name.
    name: String,
}

/// Renders an error response with a status code and a plain-text message.
fn render_error(res: &mut Response, status: StatusCode, message: impl std::fmt::Display) {
    res.status_code(status);
    res.render(Text::Plain(message.to_string()));
}

/// `GET /hello` — plain-text greeting.
#[handler]
async fn hello() -> &'static str {
    "Hello from Cloudflare Workers"
}

/// `GET /hello/{name}` — reads a path parameter.
#[handler]
async fn hello_name(req: &mut Request, res: &mut Response) {
    let name = req
        .param::<String>("name")
        .unwrap_or_else(|| "world".into());
    res.render(Text::Plain(format!("Hello, {name}!")));
}

/// `GET /greet?name=...` — reads a query parameter.
#[handler]
async fn greet(req: &mut Request, res: &mut Response) {
    let name = req
        .query::<String>("name")
        .unwrap_or_else(|| "stranger".into());
    res.render(Text::Plain(format!("Hi, {name}!")));
}

/// `POST /echo` — parses a JSON body and echoes it back as JSON.
#[handler]
async fn echo_json(req: &mut Request, res: &mut Response) {
    match req.parse_json::<EchoPayload>().await {
        Ok(payload) => res.render(Json(payload)),
        Err(err) => render_error(res, StatusCode::BAD_REQUEST, err),
    }
}

/// `GET /info` — returns a JSON overview of the runtime.
#[handler]
async fn info(res: &mut Response) {
    res.render(Json(serde_json::json!({
        "runtime": "Cloudflare Workers",
        "framework": "Salvo",
    })));
}

/// `GET /env` — reads request context and Workers bindings.
///
/// The secret is never echoed; only its presence is reported.
#[handler]
async fn env_info(req: &mut Request, res: &mut Response) {
    let greeting = req.worker_var("GREETING").map_or_else(
        |_| "missing `GREETING` var binding".to_owned(),
        |value| value.to_string(),
    );
    let secret_state = match req.worker_secret("API_KEY") {
        Ok(secret) if !secret.to_string().is_empty() => "configured",
        Ok(_) => "empty",
        Err(_) => "missing `API_KEY` secret binding",
    };
    res.render(Json(serde_json::json!({
        "remote_addr": req.remote_addr().to_string(),
        "method": req.method().to_string(),
        "scheme": req.scheme().to_string(),
        "path": req.uri().path(),
        "var_greeting": greeting,
        "secret_api_key": secret_state,
        "worker_ctx": if req.worker_ctx().is_ok() { "attached" } else { "missing" },
    })));
}

/// `GET /kv/{key}` — reads a value from the KV namespace.
#[handler]
async fn kv_get(req: &mut Request, res: &mut Response) {
    let Some(key) = req.param::<String>("key") else {
        return render_error(res, StatusCode::BAD_REQUEST, "missing <key> path parameter");
    };
    let store = match req.worker_kv("KV") {
        Ok(store) => store,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    match store.get(&key).text().into_send().await {
        Ok(Some(value)) => res.render(Text::Plain(value)),
        Ok(None) => render_error(res, StatusCode::NOT_FOUND, format!("key `{key}` not found")),
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `PUT /kv/{key}` — writes a JSON `{"value": "..."}` body into the KV namespace.
#[handler]
async fn kv_put(req: &mut Request, res: &mut Response) {
    let Some(key) = req.param::<String>("key") else {
        return render_error(res, StatusCode::BAD_REQUEST, "missing <key> path parameter");
    };
    let Ok(payload) = req.parse_json::<KvPutPayload>().await else {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            r#"expected JSON body `{"value": "..."}`"#,
        );
    };
    let store = match req.worker_kv("KV") {
        Ok(store) => store,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    match store.put(&key, payload.value) {
        Ok(builder) => {
            if let Err(err) = builder.execute().into_send().await {
                return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err);
            }
            res.render(Text::Plain(format!("stored `{key}`")));
        }
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `POST /d1/init` — idempotently creates and seeds the `users` table.
///
/// The seed rows are only inserted while the table is still empty, so this
/// endpoint is safe to call repeatedly from the browser test page.
#[handler]
async fn d1_init(req: &mut Request, res: &mut Response) {
    let db = match req.worker_d1("DB") {
        Ok(db) => db,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    // 每条种子数据按 `name` 判断是否已存在再插入，因此重复调用 /d1/init 不会
    // 产生重复行，且能补齐之前缺失的行（幂等且可自愈）。
    // Each seed row is inserted only when its `name` does not exist yet, so calling
    // /d1/init repeatedly never duplicates rows and can heal previously missing ones
    // (idempotent and self-healing).
    const MIGRATION: &str = "\
CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
INSERT INTO users (name) SELECT 'Alice' WHERE NOT EXISTS (SELECT 1 FROM users WHERE name = 'Alice');
INSERT INTO users (name) SELECT 'Bob' WHERE NOT EXISTS (SELECT 1 FROM users WHERE name = 'Bob');
INSERT INTO users (name) SELECT 'Carol' WHERE NOT EXISTS (SELECT 1 FROM users WHERE name = 'Carol');";
    match db.exec(MIGRATION).into_send().await {
        Ok(_) => res.render(Json(serde_json::json!({
            "ok": true,
            "message": "users table initialized and seeded"
        }))),
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `GET /d1/users` — lists rows from the D1 `users` table.
#[handler]
async fn d1_users(req: &mut Request, res: &mut Response) {
    let db = match req.worker_d1("DB") {
        Ok(db) => db,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    let statement = db.prepare("SELECT id, name FROM users ORDER BY id LIMIT 20");
    match statement.all().into_send().await {
        Ok(result) => match result.results::<User>() {
            Ok(users) => res.render(Json(users)),
            Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
        },
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `GET /r2/objects` — lists object keys in the R2 bucket.
#[handler]
async fn r2_objects(req: &mut Request, res: &mut Response) {
    let bucket = match req.worker_bucket("BUCKET") {
        Ok(bucket) => bucket,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    match bucket.list().execute().into_send().await {
        Ok(objects) => {
            let keys: Vec<String> = objects
                .objects()
                .into_iter()
                .map(|object| object.key())
                .collect();
            res.render(Json(
                serde_json::json!({ "count": keys.len(), "keys": keys }),
            ));
        }
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `GET /r2/objects/{key}` — streams the raw object bytes from the R2 bucket.
///
/// 响应体通过 [`StreamFile`] 流式生成：处理器不一次性加载整个对象，而是向
/// `StreamFile` 注入一个按偏移量读取的闭包，按需发起 R2 分片读取，因此任意时刻
/// 内存中只保留一个分片。
/// The response body is streamed via [`StreamFile`]: instead of loading the whole
/// object into memory, the handler injects an offset-based read closure that issues
/// ranged R2 reads on demand, so only one chunk is buffered at a time.
#[handler]
async fn r2_get(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(key) = req.param::<String>("key") else {
        return render_error(res, StatusCode::BAD_REQUEST, "missing {key} path parameter");
    };
    let bucket = match req.worker_bucket("BUCKET") {
        Ok(bucket) => bucket,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    // 先读取对象元数据以获取总大小，`head` 不下载对象体。
    // Read object metadata first to obtain the total size; `head` does not
    // download the object body.
    let object = match bucket.head(&key).into_send().await {
        Ok(Some(object)) => object,
        Ok(None) => {
            return render_error(
                res,
                StatusCode::NOT_FOUND,
                format!("object `{key}` not found"),
            );
        }
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    let total_size = object.size();

    // Workers 运行时为单线程，R2 的 `Bucket` 不满足 `Send + Sync`；用 `SendWrapper`
    // 包装后即可在 `StreamFile` 的闭包（要求 `Send + Sync`）中安全共享。
    // The Workers runtime is single-threaded and R2's `Bucket` is not `Send + Sync`;
    // wrapping it in `SendWrapper` lets the `StreamFile` closure (which requires
    // `Send + Sync`) share it safely.
    let bucket = SendWrapper::new(bucket);
    let key_for_reader = key.clone();
    let reader = move |offset: u64, size: usize| {
        let bucket = bucket.clone();
        let key = key_for_reader.clone();
        async move {
            let range = Range::OffsetWithLength {
                offset,
                length: size as u64,
            };
            let object = bucket
                .get(&key)
                .range(range)
                .execute()
                .into_send()
                .await
                .map_err(|err| std::io::Error::other(err.to_string()))?;
            let Some(object) = object else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("object `{key}` not found"),
                ));
            };
            let Some(body) = object.body() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!("object `{key}` has no body"),
                ));
            };
            let bytes = body
                .bytes()
                .into_send()
                .await
                .map_err(|err| std::io::Error::other(err.to_string()))?;
            Ok(Bytes::from(bytes))
        }
    };

    let stream_file = StreamFile::new(key.clone(), total_size, reader)
        .with_attached_name(key)
        .with_buffer_size(1024 * 1024);
    stream_file.write(req, depot, res).await;
}

/// `PUT /r2/objects/{key}` — streams the raw request body into an R2 object.
///
/// 将原始请求体以流式方式写入 R2 对象，全程不做全量内存缓冲，因此大文件上传不会
/// 触发内存溢出。R2 的 `put` 要求传入具备已知长度的流，因此先从 `Content-Length`
/// 读取总字节数，再把 Salvo 请求体包装为 Workers 的 `FixedLengthStream`。
/// The request body is forwarded chunk-by-chunk into R2 without buffering it in
/// memory. R2's `put` requires a stream with a known length, so the handler reads
/// `Content-Length` first and wraps the Salvo body in a Workers `FixedLengthStream`.
#[handler]
async fn r2_put(req: &mut Request, res: &mut Response) {
    let Some(key) = req.param::<String>("key") else {
        return render_error(res, StatusCode::BAD_REQUEST, "missing {key} path parameter");
    };
    let bucket = match req.worker_bucket("BUCKET") {
        Ok(bucket) => bucket,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    let Some(content_length) = req
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            "missing or invalid `content-length` header",
        );
    };
    // 将 Salvo 请求体转换为 `Stream<Item = Result<Vec<u8>>>`，再包装为 Workers 的
    // `FixedLengthStream`（携带已知长度），交给 R2 边读边写，避免一次性读入内存。
    // Convert the Salvo body into a `Stream<Item = Result<Vec<u8>>>` and wrap it in a
    // Workers `FixedLengthStream` (which carries the known length), so R2 can stream it
    // without buffering the whole upload in memory.
    let stream = req.take_body().into_data_stream().map(|item| {
        item.map(|bytes: Bytes| bytes.to_vec())
            .map_err(|err| worker::Error::RustError(err.to_string()))
    });
    let fixed = worker::FixedLengthStream::wrap(stream, content_length);
    match bucket.put(&key, fixed).execute().into_send().await {
        Ok(Some(object)) => res.render(Json(serde_json::json!({
            "key": object.key(),
            "size": object.size(),
            "etag": object.etag(),
        }))),
        Ok(None) => render_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "upload returned no object",
        ),
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `DELETE /r2/objects/{key}` — deletes an object from the R2 bucket.
#[handler]
async fn r2_delete(req: &mut Request, res: &mut Response) {
    let Some(key) = req.param::<String>("key") else {
        return render_error(res, StatusCode::BAD_REQUEST, "missing {key} path parameter");
    };
    let bucket = match req.worker_bucket("BUCKET") {
        Ok(bucket) => bucket,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    match bucket.delete(&key).into_send().await {
        Ok(()) => res.render(Text::Plain(format!("deleted `{key}`"))),
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `POST /queue` — enqueues a JSON message.
#[handler]
async fn enqueue(req: &mut Request, res: &mut Response) {
    let Ok(payload) = req.parse_json::<QueueMessage>().await else {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            r#"expected JSON body `{"text": "..."}`"#,
        );
    };
    let queue = match req.worker_queue("QUEUE") {
        Ok(queue) => queue,
        Err(err) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    };
    match queue.send(payload).into_send().await {
        Ok(()) => res.render(Text::Plain("message enqueued")),
        Err(err) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

/// `GET /ws` — WebSocket echo with close-frame handling.
#[handler]
async fn ws_echo(
    req: &mut Request,
    _depot: &mut Depot,
    res: &mut Response,
    _ctrl: &mut FlowCtrl,
) -> Result<(), StatusError> {
    WebSocketUpgrade::new()
        .upgrade(req, res, |mut ws| async move {
            while let Some(msg) = ws.recv().await {
                let Ok(msg) = msg else {
                    break;
                };
                // A close frame ends the loop; no reply is sent.
                if msg.is_close() {
                    break;
                }
                if ws.send(msg).await.is_err() {
                    break;
                }
            }
        })
        .await
}

/// `GET /` — serves the embedded browser test page that exercises every endpoint.
#[handler]
async fn index_page(res: &mut Response) {
    res.render(Text::Html(include_str!("../static/index.html")));
}

/// Builds the Workers router.
///
/// `#[service]` generates the `worker::event(fetch)` entry point that drives
/// this router for every incoming Workers request.
#[service]
fn build() -> Router {
    Router::new()
        .push(Router::with_path("hello").get(hello))
        .push(Router::with_path("hello/{name}").get(hello_name))
        .push(Router::with_path("greet").get(greet))
        .push(Router::with_path("echo").post(echo_json))
        .push(Router::with_path("info").get(info))
        .push(Router::with_path("env").get(env_info))
        .push(Router::with_path("kv").push(Router::with_path("{key}").get(kv_get).put(kv_put)))
        .push(
            Router::with_path("d1")
                .push(Router::with_path("init").post(d1_init))
                .push(Router::with_path("users").get(d1_users)),
        )
        .push(
            Router::with_path("r2").push(
                Router::with_path("objects").get(r2_objects).push(
                    Router::with_path("{key}")
                        .get(r2_get)
                        .put(r2_put)
                        .delete(r2_delete),
                ),
            ),
        )
        .push(Router::with_path("queue").post(enqueue))
        .push(Router::with_path("ws").get(ws_echo))
        .push(Router::with_path("/").get(index_page))
        .push(Router::with_path("index.html").get(index_page))
}
