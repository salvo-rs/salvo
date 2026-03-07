<div align="center">
<p><img alt="Salvo" width="132" style="max-width:40%;min-width:60px;" src="https://salvo.rs/images/logo-text.svg" /></p>
<p>
    <a href="https://github.com/salvo-rs/salvo/blob/main/README.md">English</a>&nbsp;&nbsp;
    <a href="https://github.com/salvo-rs/salvo/blob/main/README.zh.md">简体中文</a>&nbsp;&nbsp;
    <a href="https://github.com/salvo-rs/salvo/blob/main/README.zh-hant.md">繁體中文</a>
</p>
<p>
<a href="https://github.com/salvo-rs/salvo/actions">
    <img alt="build status" src="https://github.com/salvo-rs/salvo/workflows/ci-linux/badge.svg" />
</a>
<a href="https://github.com/salvo-rs/salvo/actions">
    <img alt="build status" src="https://github.com/salvo-rs/salvo/workflows/ci-macos/badge.svg" />
</a>
<a href="https://github.com/salvo-rs/salvo/actions">
    <img alt="build status" src="https://github.com/salvo-rs/salvo/workflows/ci-windows/badge.svg" />
</a>
<a href="https://codecov.io/gh/salvo-rs/salvo"><img alt="codecov" src="https://codecov.io/gh/salvo-rs/salvo/branch/main/graph/badge.svg" /></a>
<br>
<a href="https://crates.io/crates/salvo"><img alt="crates.io" src="https://img.shields.io/crates/v/salvo" /></a>
<a href="https://docs.rs/salvo"><img alt="Documentation" src="https://docs.rs/salvo/badge.svg" /></a>
<a href="https://crates.io/crates/salvo"><img alt="Download" src="https://img.shields.io/crates/d/salvo.svg" /></a>
<a href="https://github.com/rust-secure-code/safety-dance/"><img alt="unsafe forbidden" src="https://img.shields.io/badge/unsafe-forbidden-success.svg" /></a>
<a href="https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.89%2B-blue" /></a>
<br>
<a href="https://salvo.rs">
    <img alt="Website" src="https://img.shields.io/badge/https-salvo.rs-%23f00" />
</a>
<a href="https://discord.gg/G8KfmS6ByH">
    <img src="https://img.shields.io/discord/1041442427006890014.svg?logo=discord">
</a>
<a href="https://gitcode.com/salvo-rs/salvo">
    <img src="https://gitcode.com/salvo-rs/salvo/star/badge.svg">
</a>
</p>
</div>

Salvo is an extremely simple and powerful Rust web backend framework. Only basic Rust knowledge is required to develop backend services.

# salvo-tus

[TUS](https://tus.io/) (Resumable Upload Protocol) implementation for the Salvo web framework. TUS is an open protocol for resumable file uploads over HTTP, allowing reliable uploads of large files by enabling pause and resume functionality.

## Features

- **Resumable uploads**: Clients can resume interrupted uploads from where they left off
- **Upload metadata**: Attach custom metadata to uploads
- **Configurable max size**: Limit upload file sizes with fixed or dynamic limits
- **Lifecycle hooks**: React to upload events (create, finish, incoming request)
- **Custom upload IDs**: Generate custom upload identifiers
- **Customizable storage**: Use built-in disk storage or implement your own backend

## Protocol Support

- **TUS protocol version**: 1.0.0
- **Extensions**: creation, creation-with-upload, creation-defer-length, termination
- **Built-in handlers**: OPTIONS, POST, HEAD, PATCH, DELETE, GET

## TUS Protocol Endpoints

| Method | Path | Description |
|--------|------|-------------|
| OPTIONS | `/uploads` | Returns TUS protocol capabilities |
| POST | `/uploads` | Creates a new upload |
| HEAD | `/uploads/{id}` | Returns upload progress |
| PATCH | `/uploads/{id}` | Uploads a chunk |
| DELETE | `/uploads/{id}` | Cancels an upload |
| GET | `/uploads/{id}` | Downloads the uploaded file |

## Installation

This is an official crate, so you can enable it in `Cargo.toml`:

```toml
salvo = { version = "*", features = ["tus"] }
```

## Quick Start

```rust
use salvo::prelude::*;
use salvo::tus::{Tus, MaxSize};

#[tokio::main]
async fn main() {
    let tus = Tus::new()
        .path("/uploads")
        .max_size(MaxSize::Fixed(100 * 1024 * 1024));  // 100 MB limit

    let router = Router::new()
        .push(tus.into_router());

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

## Lifecycle Hooks

React to upload events:

```rust
let tus = Tus::new()
    .with_on_upload_create(|req, upload_info| async move {
        println!("New upload: {:?}", upload_info);
        Ok(UploadPatch::default())
    })
    .with_on_upload_finish(|req, upload_info| async move {
        println!("Upload complete: {:?}", upload_info);
        Ok(UploadFinishPatch::default())
    });
```

## Storage Backends

By default, files are stored on disk using `DiskStore`. Implement the `DataStore` trait for custom storage (S3, database, etc.).

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-tus)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)
- [TUS Protocol Specification](https://tus.io/protocols/resumable-upload)
