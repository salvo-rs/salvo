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
<a href="https://blog.rust-lang.org/2025/12/11/Rust-1.92.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.92%2B-blue" /></a>
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

# salvo-jwt-auth

JWT (JSON Web Token) authentication middleware for the Salvo web framework.

## Features

- **Flexible token extraction**: Extract tokens from headers, query parameters, cookies, or form data
- **Multiple authentication strategies**: Use either static keys or OpenID Connect for token validation
- **Easy integration**: Works seamlessly within Salvo's middleware system
- **Type-safe claims**: Decode tokens into your own custom claims structs
- **Configurable validation**: Customize token validation rules

## Installation

This is an official crate, so you can enable it in `Cargo.toml`:

```toml
salvo = { version = "*", features = ["jwt-auth"] }
```

## Quick Start

Use `HeaderFinder` for the standard `Authorization: Bearer <token>` header.
`force_passed(true)` lets the request reach your handler so the handler can
decide how to respond to authorized, unauthorized, and forbidden states.

```rust
use salvo::jwt_auth::{ConstDecoder, HeaderFinder, JwtAuthState};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

const SECRET: &[u8] = b"replace-with-a-secret-from-your-config";

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Claims {
    sub: String,
    exp: i64,
}

#[handler]
async fn me(depot: &mut Depot, res: &mut Response) {
    match depot.jwt_auth_state() {
        JwtAuthState::Authorized => {
            let data = depot.jwt_auth_data::<Claims>().unwrap();
            res.render(Json(&data.claims));
        }
        JwtAuthState::Unauthorized => res.render(StatusError::unauthorized()),
        JwtAuthState::Forbidden => res.render(StatusError::forbidden()),
    }
}

#[tokio::main]
async fn main() {
    let auth = JwtAuth::<Claims, _>::new(ConstDecoder::from_secret(SECRET))
        .finders(vec![Box::new(HeaderFinder::new())])
        .force_passed(true);

    let router = Router::new().hoop(auth).get(me);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

Avoid query-string tokens in production because URLs are commonly saved in
browser history, logs, and referrer headers. Prefer Authorization headers or
secure, `HttpOnly` cookies.

## Audience (`aud`) claim validation

`JwtAuth` supports the JWT `aud` claim through the decoder that you attach to
middleware. The middleware extracts the token and stores the decoded claims, but
claim validation is performed by the configured `JwtAuthDecoder`.

For static keys, use `ConstDecoder::with_validation` when your service needs to
accept tokens only for a specific audience. Configure `Validation` before
constructing the decoder:

```rust
use salvo::jwt_auth::{Algorithm, ConstDecoder, DecodingKey, Validation};

let mut validation = Validation::new(Algorithm::HS256);
validation.set_audience(&["api://salvo-service"]);
validation.required_spec_claims.insert("aud".to_owned());

let decoder = ConstDecoder::with_validation(
    DecodingKey::from_secret(SECRET),
    validation,
);
```

For OpenID Connect, `OidcDecoder::new(issuer, audience)` is the shortest path.
It configures issuer validation and marks `aud` as required for the expected
audience:

```rust
use salvo::jwt_auth::OidcDecoder;

let decoder = OidcDecoder::new(
    "https://issuer.example.com",
    "api://salvo-service",
).await?;
```

If you need to accept more than one audience, build the OIDC decoder explicitly:

```rust
let decoder = OidcDecoder::builder("https://issuer.example.com")
    .audiences(["api://salvo-service", "api://salvo-admin"])
    .build()
    .await?;
```

Troubleshooting audience failures:

- Make sure the token actually contains an `aud` claim. `OidcDecoder::new` and
  `.audiences(...)` require it automatically; for static keys, add `aud` to
  `required_spec_claims` if the claim must be present.
- Match the exact audience string issued by your identity provider, including
  prefixes such as `api://` when they are part of the configured audience.
- For OIDC tokens, pass the application/API audience, not the issuer URL. The
  issuer is validated separately from `aud`.
- If your identity provider issues multiple audiences, configure every accepted
  value with `audiences(...)` or use a custom decoder for more complex policy.
- If a token decodes without audience validation but fails after enabling it,
  inspect the provider's JWT template or API settings first; Salvo is comparing
  the decoded `aud` value against the validation configuration.

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-jwt-auth)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under [Apache License, Version 2.0](LICENSE) ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).
