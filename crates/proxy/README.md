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
<a href="https://docs.rs/salvo"><img alt="Documentation" src="https://img.shields.io/docsrs/salvo.svg" /></a>
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

# salvo-proxy

## Proxy middleware for Salvo

This crate provides proxy capabilities for the Salvo web framework, allowing you to forward requests to upstream servers. It's useful for creating API gateways, load balancers, and reverse proxies.

### Features

- Support for HTTP and HTTPS proxying
- Multiple upstream server selection strategies
- WebSocket connection support
- Header manipulation
- Path and query rewriting
- Multiple HTTP client backends (Hyper, Reqwest)

### Usage

This is an official crate, so you can enable it in `Cargo.toml` like this:

```toml
salvo = { version = "*", features = ["proxy"] }
```

### Path forwarding

The default path getter forwards the wildcard tail captured by routes such as
`Router::with_path("api/{**rest}")`. If the upstream is `http://backend`, the
gateway path `/api/users` is forwarded to `http://backend/users`. If the backend
also expects the `/api` prefix, configure the upstream with that base path:
`http://backend/api`.

`Proxy` uses strict path normalization by default. It rejects literal `..` path
segments and ambiguous encoded path characters before forwarding, then
normalizes safe paths. This is useful when the proxy is used as an
access-control boundary and the upstream may perform an additional URL decode or
normalization pass:

```rust
use salvo::prelude::*;
use salvo::proxy::Proxy;

let router = Router::with_path("api/{**rest}").goal(
    Proxy::use_hyper_client("http://backend/api")
        .strict_path_normalization(true),
);
```

The default path getter re-encodes each decoded routing segment independently,
using the boundaries retained by `req.params().tail_segments()`. A slash decoded
from `%2F` stays encoded within one segment; literal percent signs become `%25`.
This also applies when strict normalization is disabled. For example,
`/_matrix/profile/@a%2Fb:example.org` can be forwarded with the ID in one segment
after an explicit opt-out for a backend that supports this interpretation.

Strict path normalization rejects literal parent-directory components and
remaining escapes for NUL, `.`, `/`, `\`, and unreserved characters (letters, digits,
`-`, `_`, `~`). A percent sign that could introduce another escape is also
rejected: `/files/a%2541.txt`, `/%2572eport_note/secret.txt`, and `/files/x%2500y`
return `400`. Literal percent signs without two following hex digits remain
usable, as in `/files/50%25off.txt` and `/files/100%25`.

A custom `url_path_getter` still supplies the path with any intended escapes
already present; the proxy leaves its `%` characters unchanged. Return a literal
percent sign as `%25`. The same strict checks apply to custom getters, and
`strict_path_normalization(false)` allows their escapes to pass through.
Backends should still validate and normalize their own routes and file paths;
this option only prevents common proxy/backend path interpretation mismatches.

The original inbound request URI is unchanged. For Matrix federation and other
signed requests, verify signatures against its original `path_and_query()` before
forwarding. The proxy may change the mount prefix and encoded spelling, so its
rewritten upstream target is not interchangeable with the signed inbound target.
URL normalization is independent of Windows filesystem naming rules; backends
must perform their own resource authorization and filesystem validation.

### Forwarded headers

The default host header getter (`standard_host_header_getter`) forwards the
upstream host and includes a non-default port, for example `example.com:8080`
(per RFC 7230 / RFC 9110). Configure `default_host_header_getter` if you want to
forward only the bare host name without the port.

Client IP forwarding overwrites any inbound `X-Forwarded-For` value with the
direct client IP from the connection. This avoids trusting a client-supplied
forwarding chain. When the peer has no IP address, such as a Unix socket, the
header is omitted instead of forwarding the inbound value.

### Request bodies

Unread request bodies are forwarded as streams. If middleware reads `req.payload()`
first, the proxy reuses those cached bytes. It derives `Content-Length` from the
forwarded body when its length is known; otherwise the HTTP client chooses the
stream framing.

URL-encoded forms read by `req.form()`, `req.form_data()` or a CSRF `FormFinder`
retain their raw bytes, so the proxy forwards the original encoding and fields.
Multipart parsing streams directly without retaining the raw body. Read
`req.payload()` before parsing multipart forms if the request will be proxied. Requests
whose body has been consumed by a parsing helper without recoverable bytes are
rejected, including after a failed or cancelled read. Retrying a parsing helper
cannot recover the consumed stream; an earlier size-limit failure remains a
`PayloadTooLarge` error on subsequent reads. Calling `take_body()` or borrowing
`body_mut()` does not clear that failure. Install a new body to recover.
Direct reads through `req.body_mut()` are not
cached; middleware doing so must replace the body with the bytes to forward.
To replace a body with an empty one intentionally, use
`req.replace_body(ReqBody::from(""))`.

### Timeout phases and streaming

The inbound server fuse, handler deadline, and upstream HTTP client cover different phases.
`Timeout` middleware bounds downstream handler processing, including waiting for upstream
response headers. It stops when the proxy has installed the streaming body in the response.
It does not bound the remainder of that body or an upgraded WebSocket tunnel.

For finite upstream responses, configure the HTTP client as well. With the `reqwest-client`
feature, this example sets separate connect, per-read, and total request deadlines:

```rust,no_run
use std::time::Duration;
use salvo_proxy::{Proxy, ReqwestClient};

fn finite_proxy() -> Result<Proxy<&'static str, ReqwestClient>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build()?;
    Ok(Proxy::new("https://backend.example", ReqwestClient::new(client)))
}
```

The connect timeout bounds connection establishment. The read timeout resets after a
successful upstream read and covers stalled reads, including body reads. The total timeout
covers the upstream request through body completion. `HyperClient` users must configure
an appropriate connector and, for deadlines beyond connection setup, a custom client/body
wrapper; the default Hyper client does not add these three application deadlines.

For SSE or other intentional long streams, use a separate client policy without a short
total request timeout, and choose a read timeout longer than the expected heartbeat interval.
A handler deadline can still bound stream setup. Configure WebSocket tunnel lifetime and
heartbeat policy separately after upgrade. Inbound `FuseConfig::request_body_timeout` limits
request-body gaps, while TCP `connection_idle_timeout` and `write_stall_timeout` operate on
the connection, not one response. They do not isolate response-body inactivity across HTTP/2
streams; HTTP/3 uses QUIC transport controls. `ResBody::observe` reports body production,
not network delivery, and does not enforce an idle or total timeout. Once response headers
are sent, body errors terminate the stream rather than produce a new error response.

### Unix socket upstreams

Use `Proxy::new("http://unix/", UnixSockClient::with_socket_path("/run/app.sock.d/server.sock"))`
to configure the socket independently of the upstream HTTP path and query. This
also supports directories whose names end in `.sock`. The legacy unit client
infers the socket from the first complete path component ending in `.sock`.
Both clients validate Upgrade selections and retain bidirectional tunnels in the
server's connection lifecycle, including forceful and graceful shutdown.

### Response header ownership

The upstream response body replaces any prepared body. Its representation fields
(including length, type, encoding, validators and digests) replace old metadata;
fields absent from the upstream are cleared. Other upstream fields replace
prepared values of the same name. Set-Cookie and Vary remain additive, and
unrelated outer headers are preserved. Apply an outer policy after the proxy
returns when it must override upstream response fields.

### Failure logging

Upstream request failures log the request method, path, and client error type.
Query strings and client error messages are excluded because they can contain
signed URLs, tokens, or other credentials. Custom clients can add their own
sanitized diagnostics when more detail is required.

[![Docs](https://img.shields.io/docsrs/salvo-proxy.svg)](https://docs.rs/salvo-proxy)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under [Apache License, Version 2.0](LICENSE) ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).
