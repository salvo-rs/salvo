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
<a href="https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.95%2B-blue" /></a>
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

## Absolute Location URLs

Tus returns relative `Location` headers by default. If clients need absolute
upload URLs in production, set a fixed trusted origin with
`absolute_location`:

```rust
let tus = Tus::new()
    .path("/uploads")
    .absolute_location("https://uploads.example.com");
```

Avoid using `relative_location(false)` by itself for internet-facing services:
without a canonical origin, absolute URLs are derived from request `Host` or
forwarded headers.

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

## Binary Upload Metadata and Migration

`Metadata` stores decoded values as `HashMap<String, Option<Vec<u8>>>` so arbitrary
binary values survive creation, persistence, and `HEAD` responses. TUS headers
continue to use base64. `None` represents a key without a value;
`Some(Vec::new())` represents an explicitly empty value.

**Breaking Rust API change:** metadata values were previously `Option<String>`.
Convert text when inserting it, and use `get_text` when a field is expected to be
UTF-8:

```rust
use salvo::tus::Metadata;

let mut metadata = Metadata::default();
metadata.insert("filename".to_owned(), Some("report.pdf".as_bytes().to_vec()));
metadata.insert("checksum".to_owned(), Some(vec![0xff, 0, 0x80]));
metadata.insert("flag".to_owned(), None);

assert_eq!(metadata.get_text("filename").unwrap(), Some("report.pdf"));
assert!(metadata.get_text("checksum").is_err());
```

For an owned `String`, replace `Some(text)` with `Some(text.into_bytes())`.
`get_text` returns an error for invalid UTF-8 without changing the raw bytes;
missing keys and valueless keys return `Ok(None)`, while an explicitly empty
value returns `Ok(Some(""))`. Direct map access now returns byte vectors.
Custom `DataStore` implementations, upload hooks, naming callbacks, and
`UploadPatch` producers must adapt their metadata values and persistence formats.
The built-in filename and filetype hints use strict UTF-8: binary values remain
metadata but are ignored for naming.

`DiskStore` reads both the previous JSON string values and new JSON byte arrays
inside its separated `metadata` directory. New records and subsequent updates
write arrays, including for valid UTF-8 text; valueless keys remain `null` and
explicitly empty values use `[]`. Reading an old record alone does not rewrite it.
Previously corrupted binary values cannot be reconstructed from old strings.

**Disk format downgrade limit:** older versions cannot read the new byte arrays.
Stop older readers and writers before upgrading a shared store. Keep a backup
before the first new write. Downgrading requires an explicit conversion of
valid UTF-8 values back to strings; arbitrary binary values cannot be represented
losslessly in the old format. This compatibility reader does not import legacy
flat-layout uploads or change the existing directory trust boundary.

## Storage Backends

By default, files are stored on disk using `DiskStore`. Implement the `DataStore` trait for custom storage (S3, database, etc.).

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-tus)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)
- [TUS Protocol Specification](https://tus.io/protocols/resumable-upload)
