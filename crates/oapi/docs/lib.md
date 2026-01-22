OpenAPI support for the Salvo web framework.

This crate provides automatic OpenAPI documentation generation using simple
procedural macros. Annotate your handlers and types, and get a complete OpenAPI
specification.

# Quick Start

1. Add the `oapi` feature to your Salvo dependency
2. Use `#[endpoint]` instead of `#[handler]` on your handlers
3. Derive `ToSchema` on your data types
4. Create an `OpenApi` instance and mount the Swagger UI

```ignore
use salvo::prelude::*;
use salvo::oapi::extract::*;

#[derive(ToSchema, serde::Deserialize)]
struct User {
    id: i64,
    name: String,
}

#[endpoint]
async fn get_user(id: PathParam<i64>) -> Json<User> {
    Json(User { id: *id, name: "Alice".into() })
}

#[tokio::main]
async fn main() {
    let router = Router::new()
        .push(Router::with_path("users/<id>").get(get_user));

    let doc = OpenApi::new("My API", "1.0.0").merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("/swagger-ui"));

    let acceptor = TcpListener::new("127.0.0.1:8080").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

# Key Traits

| Trait | Purpose | Derive Macro |
|-------|---------|--------------|
| [`ToSchema`] | Define JSON schema for types | `#[derive(ToSchema)]` |
| [`ToParameters`] | Define query/path parameters | `#[derive(ToParameters)]` |
| [`ToResponse`] | Define a single response type | `#[derive(ToResponse)]` |
| [`ToResponses`] | Define multiple response types | `#[derive(ToResponses)]` |

# Documentation UIs

Multiple OpenAPI documentation UIs are available:

| UI | Feature Flag | Description |
|----|--------------|-------------|
| Swagger UI | `swagger-ui` | Interactive API explorer |
| Scalar | `scalar` | Modern, beautiful API docs |
| RapiDoc | `rapidoc` | Customizable API documentation |
| ReDoc | `redoc` | Clean, responsive documentation |

# Crate Features

## Serialization

- **`yaml`** - Enable YAML serialization of OpenAPI objects

## Type Support

- **`chrono`** - Support for `chrono` date/time types (`DateTime`, `Date`, `NaiveDate`, `Duration`)
  - `DateTime` uses `format: date-time` per [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14)
  - `Date` and `NaiveDate` use `format: date`

- **`time`** - Support for `time` crate types (`OffsetDateTime`, `PrimitiveDateTime`, `Date`, `Duration`)

- **`decimal`** - Support for `rust_decimal::Decimal` as String (default)

- **`decimal-float`** - Support for `rust_decimal::Decimal` as Number (mutually exclusive with `decimal`)

- **`uuid`** - Support for `uuid::Uuid` with `format: uuid`

- **`ulid`** - Support for `ulid::Ulid` with `format: ulid`

- **`url`** - Support for `url::Url` with `format: uri`

- **`smallvec`** - Support for `SmallVec` (rendered as array)

- **`indexmap`** - Support for `IndexMap` (rendered as object)

# Examples

Browse the [examples directory](https://github.com/salvo-rs/salvo/tree/master/examples)
for comprehensive examples including:

- `oapi-hello` - Basic OpenAPI setup
- `oapi-todos` - CRUD API with documentation
- `oapi-upload` - File upload documentation

# Learn More

- [`derive@ToSchema`] - Schema derivation with all attributes
- [`derive@ToParameters`] - Parameter extraction documentation
- [`derive@ToResponse`] / [`derive@ToResponses`] - Response documentation
- [`endpoint`] - Endpoint macro documentation
- [Security documentation][security] - API authentication setup

[security]: openapi/security/index.html
[to_schema_derive]: derive.ToSchema.html
