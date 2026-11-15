# Temporary SQLx SQLite compatibility patch

This is the published `sqlx-sqlite` 0.9.0 source from crates.io, upstream
`launchbadge/sqlx` commit `003b698e99e024f3621b8043a2426fde5b741171`, path
`sqlx-sqlite`. The original version and MIT/Apache-2.0 licenses are preserved.
Production Rust sources are unchanged. The crate archive stores licenses as
relative symlink text; the vendored LICENSE-MIT and LICENSE-APACHE contain the
complete root license texts from that same upstream commit.

The only manifest change requires `libsqlite3-sys = "0.39"` instead of
`>=0.30.1, <0.38.0`. The old upper bound cannot select SQLite 3.54.0, fixed for
RUSTSEC-2026-0459. The examples workspace applies this patch explicitly and its
SQLite in-memory smoke test checks the actual linked engine version and CRUD.
This does not modify SQLx TLS features or disable SQLite support.

Remove this directory and the workspace patch once a supported SQLx release
accepts libsqlite3-sys >=0.39. Refresh the lockfile, run the SQLite smoke test,
check all SQLx/SeaORM examples, and audit the complete examples resolution when
removing or refreshing the patch. Review the upstream diff before copying a
new release; do not silently widen unrelated dependencies here.

The local `.gitattributes` preserves the upstream Markdown hard-line-break
whitespace in `src/connection/deserialize.rs`; other files retain normal
whitespace checks. This does not alter any upstream production source bytes.
