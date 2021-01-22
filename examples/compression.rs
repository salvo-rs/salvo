#![deny(warnings)]

use salvo_core::prelude::*;
use salvo_extra::compression;
use salvo_extra::serve::*;

#[tokio::main]
async fn main() {
    let router = Router::new().push(
        Router::new().after(compression::gzip()).path("ws_chat").get(StaticFile::new("examples/ws_chat.rs"))
    ).push(
        Router::new().after(compression::deflate()).path("sse_chat").get(StaticFile::new("examples/sse_chat.rs"))
    ).push(
        Router::new().after(compression::brotli()).path("todos").get(StaticFile::new("examples/todos.rs"))
    ).push(
        Router::new().after(compression::deflate()).path("examples/<*path>").get(StaticDir::new("examples/"))
    );
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
