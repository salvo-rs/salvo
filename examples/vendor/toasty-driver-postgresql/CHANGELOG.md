# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.7.0...toasty-driver-postgresql-v0.8.0) - 2026-07-06

### Added

- Emit one toasty::query event per statement and propagate caller spans ([#1071])
- Infer `key` and `references` in `#[belongs_to]` ([#1063])

[#1063]: https://github.com/tokio-rs/toasty/pull/1063
[#1071]: https://github.com/tokio-rs/toasty/pull/1071

## [0.7.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.6.1...toasty-driver-postgresql-v0.7.0) - 2026-05-29

### Added

- Raw SQL execution API ([#965])
- Turso driver with TransactionMode-aware concurrent writes ([#938])
- TransactionMode for SQLite lock-acquisition control ([#931])
- tokio-postgres-rustls 0.14 ([#952])

### Fixed

- PostgreSQL now accepts libpq query-param connection options ([#985])

### Changed

- [**breaking**] Require Deferred relation fields ([#954])
- [**breaking**] Move schema diff types to `schema::diff` ([#929])

[#929]: https://github.com/tokio-rs/toasty/pull/929
[#931]: https://github.com/tokio-rs/toasty/pull/931
[#938]: https://github.com/tokio-rs/toasty/pull/938
[#952]: https://github.com/tokio-rs/toasty/pull/952
[#954]: https://github.com/tokio-rs/toasty/pull/954
[#965]: https://github.com/tokio-rs/toasty/pull/965
[#985]: https://github.com/tokio-rs/toasty/pull/985

## [0.6.1](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.6.0...toasty-driver-postgresql-v0.6.1) - 2026-05-16

- Internal improvements only.

## [0.6.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.5.0...toasty-driver-postgresql-v0.6.0) - 2026-05-14

### Added

- Add Vec<scalar> model field support to MySQL, SQLite, and DynamoDB ([#872])
- Add Vec<scalar> support to PostgreSQL using native array storage ([#866])
- Detect broken pool connections before the next query ([#874])
- Automatically evict stale connections after backend restart ([#867])
- Improve PostgreSQL IN-list query performance by using array parameter binding ([#818])

[#818]: https://github.com/tokio-rs/toasty/pull/818
[#866]: https://github.com/tokio-rs/toasty/pull/866
[#867]: https://github.com/tokio-rs/toasty/pull/867
[#872]: https://github.com/tokio-rs/toasty/pull/872
[#874]: https://github.com/tokio-rs/toasty/pull/874

## [0.5.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.4.0...toasty-driver-postgresql-v0.5.0) - 2026-04-27

### Added

- Support for floating-point data types ([#687])
- Native database enum types for embedded enums ([#665])
- PostgreSQL client application name configuration ([#680])

[#665]: https://github.com/tokio-rs/toasty/pull/665
[#680]: https://github.com/tokio-rs/toasty/pull/680
[#687]: https://github.com/tokio-rs/toasty/pull/687

## [0.4.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.3.0...toasty-driver-postgresql-v0.4.0) - 2026-04-11

### Added

- add opinionated support for postgres TLS using rustls ([#663](https://github.com/tokio-rs/toasty/pull/663))

### Fixed

- percent encoding in postgres connection strings ([#671](https://github.com/tokio-rs/toasty/pull/671))

## [0.3.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.2.0...toasty-driver-postgresql-v0.3.0) - 2026-04-03

### Other

- signature change on Connection trait ([#626](https://github.com/tokio-rs/toasty/pull/626))
- push pagination handling into engine ([#610](https://github.com/tokio-rs/toasty/pull/610))
- add badges to README ([#606](https://github.com/tokio-rs/toasty/pull/606))
- update README examples to use create! macro syntax ([#603](https://github.com/tokio-rs/toasty/pull/603))

## [0.2.0](https://github.com/tokio-rs/toasty/compare/toasty-driver-postgresql-v0.0.0...toasty-driver-postgresql-v0.2.0) - 2026-03-30

### Added

- add tracing-based logging across the ORM ([#586](https://github.com/tokio-rs/toasty/pull/586))
- support auto incrementing IDs ([#192](https://github.com/tokio-rs/toasty/pull/192))

### Other

- upgrade dependencies ([#598](https://github.com/tokio-rs/toasty/pull/598))
- configure readme field in workspace package metadata ([#597](https://github.com/tokio-rs/toasty/pull/597))
- organize imports and format code for consistency ([#595](https://github.com/tokio-rs/toasty/pull/595))
- update README status from Incubating to Preview ([#594](https://github.com/tokio-rs/toasty/pull/594))
- add comprehensive documentation to all database drivers ([#541](https://github.com/tokio-rs/toasty/pull/541))
- remove async_trait reexport from toasty-core ([#539](https://github.com/tokio-rs/toasty/pull/539))
- remove unsafe ([#480](https://github.com/tokio-rs/toasty/pull/480))
- fix API docs link and redesign crate index page ([#472](https://github.com/tokio-rs/toasty/pull/472))
- rename terminal query method .all() to .exec() ([#471](https://github.com/tokio-rs/toasty/pull/471))
- add nightly API docs link to README ([#469](https://github.com/tokio-rs/toasty/pull/469))
- add developer guide link to README ([#466](https://github.com/tokio-rs/toasty/pull/466))
- remove Cursor type and return Vec directly from query execution ([#448](https://github.com/tokio-rs/toasty/pull/448))
- add compile testing for documentation code snippets ([#446](https://github.com/tokio-rs/toasty/pull/446))
- rm Arc from Schema.db field. ([#387](https://github.com/tokio-rs/toasty/pull/387))
- Add transaction error variants ([#377](https://github.com/tokio-rs/toasty/pull/377))
- wrap multi-op ExecPlan in BEGIN...COMMIT for atomicity ([#370](https://github.com/tokio-rs/toasty/pull/370))
- cleanup llm context files ([#365](https://github.com/tokio-rs/toasty/pull/365))
- Remove Id type ([#334](https://github.com/tokio-rs/toasty/pull/334))
- Add database reset functionality (+ serial tests) ([#322](https://github.com/tokio-rs/toasty/pull/322))
- Use batch execute for applying Postgres migrations ([#321](https://github.com/tokio-rs/toasty/pull/321))
- Support "postgres" URL scheme for PostgreSQL ([#320](https://github.com/tokio-rs/toasty/pull/320))
- Add database migration CLI tool ([#271](https://github.com/tokio-rs/toasty/pull/271))
- move Capability to Driver ([#300](https://github.com/tokio-rs/toasty/pull/300))
- remove anyhow dependency ([#297](https://github.com/tokio-rs/toasty/pull/297))
- misc tweaks ([#295](https://github.com/tokio-rs/toasty/pull/295))
- add a custom error type ([#279](https://github.com/tokio-rs/toasty/pull/279))
- move more tests to integration suite. ([#265](https://github.com/tokio-rs/toasty/pull/265))
- Add LRU statement caching to SQL drivers ([#264](https://github.com/tokio-rs/toasty/pull/264))
- Add database connection pooling ([#260](https://github.com/tokio-rs/toasty/pull/260))
- Clean up Postgres driver Value conversions a bit ([#258](https://github.com/tokio-rs/toasty/pull/258))
- Refactor and unify driver Value handling ([#256](https://github.com/tokio-rs/toasty/pull/256))
- Add native support for Postgres NUMERIC and MySQL DECIMAL with rust_decimal ([#248](https://github.com/tokio-rs/toasty/pull/248))
- Add basic support for bigdecimal::BigDecimal ([#238](https://github.com/tokio-rs/toasty/pull/238))
- remove auto-mapping many models to one table ([#225](https://github.com/tokio-rs/toasty/pull/225))
- Update readme ([#230](https://github.com/tokio-rs/toasty/pull/230))
- Add date/time times using `jiff` ([#201](https://github.com/tokio-rs/toasty/pull/201))
- Allow specifying more storage types, e.g. `TEXT` for UUIDs ([#181](https://github.com/tokio-rs/toasty/pull/181))
- Add support for UUIDs using the uuid crate ([#178](https://github.com/tokio-rs/toasty/pull/178))
- move rest of planner to new engine. ([#167](https://github.com/tokio-rs/toasty/pull/167))
- Include association eager-loading when lowering statement ([#159](https://github.com/tokio-rs/toasty/pull/159))
- update readme to align with the working code ([#112](https://github.com/tokio-rs/toasty/pull/112))
- reduce glob imports in rest of crates ([#133](https://github.com/tokio-rs/toasty/pull/133))
- Add support for unsigned types ([#122](https://github.com/tokio-rs/toasty/pull/122))
- Add support for i16 ([#116](https://github.com/tokio-rs/toasty/pull/116))
- Support i8 ([#115](https://github.com/tokio-rs/toasty/pull/115))
- add support for i32 types ([#113](https://github.com/tokio-rs/toasty/pull/113))
- Initial pagination implementation ([#111](https://github.com/tokio-rs/toasty/pull/111))
- Add annotation to specify DB column type ([#104](https://github.com/tokio-rs/toasty/pull/104))
- Flatten capability struct and DRY db definitions ([#102](https://github.com/tokio-rs/toasty/pull/102))
- remove update condition logic from drivers ([#99](https://github.com/tokio-rs/toasty/pull/99))
- switch `driver::Rows::Count` type to u64 ([#98](https://github.com/tokio-rs/toasty/pull/98))
- complete driver, include in CI ([#97](https://github.com/tokio-rs/toasty/pull/97))
- Refactor sql serializer ([#95](https://github.com/tokio-rs/toasty/pull/95))
- move crates to flatter structure ([#91](https://github.com/tokio-rs/toasty/pull/91))
- Switch Toasty to use proc macros for schema declaration ([#76](https://github.com/tokio-rs/toasty/pull/76))
- Fix typo in README.md ([#4](https://github.com/tokio-rs/toasty/pull/4))
- Initial commit
