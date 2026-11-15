# rbdc

[![Build Status](https://github.com/rbatis/rbdc/workflows/ci/badge.svg)](https://github.com/rbatis/rbdc/actions)
[![doc.rs](https://docs.rs/rbdc/badge.svg)](https://docs.rs/rbdc/)
[![Crates.io](https://img.shields.io/crates/v/rbdc)](https://crates.io/crates/rbdc)
[![crates.io downloads](https://img.shields.io/crates/d/rbdc)](https://crates.io/crates/rbdc)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![codecov](https://codecov.io/gh/rbatis/rbdc/graph/badge.svg?token=VAVPXSHoff)](https://codecov.io/gh/rbatis/rbdc)
[![GitHub release](https://img.shields.io/github/v/release/rbatis/rbdc)](https://github.com/rbatis/rbdc/releases)
[![discussions](https://img.shields.io/github/discussions/rbatis/rbdc)](https://github.com/rbatis/rbdc/discussions)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

Database driver abstraction layer for Rust, providing a unified interface for [rbatis](https://github.com/rbatis/rbatis).

## Table of Contents

- [Features](#features)
- [Supported Databases](#supported-databases)
- [Quick Start](#quick-start)
- [Scan Utility](#scan-utility)
- [Implement Custom Driver](#implement-custom-driver)
- [License](#license)

## Features

- **Safe**: `#![forbid(unsafe_code)]` - 100% safe Rust
- **Async**: Native async support based on Tokio
- **Extensible**: Simple trait definitions for easy driver implementation

## Supported Databases

| Database | Driver |
|----------|--------|
| MySQL | [rbdc-mysql](https://crates.io/crates/rbdc-mysql) |
| PostgreSQL | [rbdc-pg](https://crates.io/crates/rbdc-pg) |
| SQLite | [rbdc-sqlite](https://crates.io/crates/rbdc-sqlite) |
| MSSQL | [rbdc-mssql](https://crates.io/crates/rbdc-mssql) |
| Truso | [rbdc-turso](https://crates.io/crates/rbdc-turso) |
| DuckDB | [rbdc-duckdb](https://crates.io/crates/rbdc-duckdb) |

## Quick Start

```rust
use rbdc_sqlite::SqliteDriver;
use rbdc_pool_fast::FastPool;

#[tokio::main]
async fn main() -> Result<(), rbdc::Error> {
    let pool = FastPool::new_url(SqliteDriver {}, "sqlite://target/test.db")?;
    let mut conn = pool.get().await?;
    let v = conn.exec_decode("SELECT * FROM sqlite_master", vec![]).await?;
    println!("{}", v);
    // if need decode use `let result: Vec<Table> = rbs::from_value(v)?;`
    Ok(())
}
```

## Scan Utility

For memory-efficient row-by-row iteration instead of loading all rows into a `Value` array at once:

```rust
use rbdc::db::Connection;
use rbdc::util::Scan;

let rows = conn.exec_rows("SELECT * FROM activity", vec![]).await?;
let scan = Scan::new(rows);

// Collect all rows into a Vec of struct
#[derive(serde::Deserialize)]
struct Activity {
    id: Option<String>,
    name: Option<String>,
}
let activities: Vec<Activity> = scan.collect()?;
```

## Implement Custom Driver

Implement these 6 traits:

```rust
use rbdc::db::{Driver, MetaData, Row, Connection, ConnectOptions, Placeholder};

impl Driver for YourDriver {}
impl MetaData for YourMetaData {
    // TODO: impl methods
}
impl Row for YourRow {
    // TODO: impl methods
}
impl Connection for YourConnection {
    // TODO: impl methods
}
impl ConnectOptions for YourConnectOptions {
    // TODO: impl methods
}
impl Placeholder for YourPlaceholder {
    // TODO: impl methods
}

/// use your driver
#[tokio::main]
async fn main() -> Result<(), rbdc::Error> {
    let uri = "YourDriver://****";
    let pool = FastPool::new_url(YourDriver {}, uri)?;
    let mut conn = pool.get().await?;
    let v = conn.exec_decode("SELECT 1", vec![]).await?;
    println!("{}", v);
}
```

For databases with blocking APIs, refer to `rbdc-sqlite` which uses the `flume` channel library.

See [examples](./examples) for more.

## License

MIT
