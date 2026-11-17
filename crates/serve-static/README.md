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

# salvo-serve-static

## Serve static assets for Salvo

This is an official crate, so you can enable it in `Cargo.toml` like this:

```toml
salvo = { version = "*", features = ["serve-static"] }
```

## Choose a filesystem boundary

Keep public and private assets in disjoint filesystem roots whenever possible. Apply your
existing authentication hoop to the private mount:

```rust
use salvo::prelude::*;
use salvo::serve_static::StaticDir;

fn asset_routes(require_login: impl Handler) -> Router {
    Router::new()
        .push(
            Router::with_path("public/{*path}")
                .goal(StaticDir::new("assets/public")),
        )
        .push(
            Router::with_path("private/{*path}")
                .hoop(require_login)
                .goal(StaticDir::new("assets/private")),
        )
}
```

`require_login` is the application's existing hoop that verifies credentials and stops
unauthenticated requests. A hoop protects requests routed through it. `StaticDir` cannot
infer the authorization rules on another route: mounting a parent directory elsewhere can
publish a private descendant through that other mount.

URL route parameters may contain data such as a Matrix ID `@a/b:example.org`,
encoded as `@a%2Fb:example.org`. Static resource handlers apply a narrower policy:
`StaticDir` and `StaticEmbed` reject `/`, backslash or NUL inside any decoded
segment with **400 Bad Request**, before file lookup, defaults or fallbacks.
Windows colon-bearing and reserved-device components are rejected at this resource
boundary too. Disabling strict filename spelling does not disable these checks.

Custom wildcard consumers should use `req.params().tail_segments()` to retain the
router's segment boundaries. Splitting the decoded `tail()` string again loses
the distinction between data and path structure. `tail_raw_segments()` exposes
the encoded segment spellings; dot and empty-segment normalization may already
have occurred. Signature verification must use the original request URI's
`path_and_query()` instead. Routing does not rewrite that URI.

`StaticDir` now uses these security defaults, which can change behavior for existing mounts:

- `strict_path_check(true)` requires the exact spelling of every filesystem component. On
  Windows this rejects alternate case, trailing-dot aliases and 8.3 short names instead of
  treating them as another URL for the same resource.
- `symlink_policy(SymlinkPolicy::Deny)` rejects symbolic links and Windows junctions below
  the configured root, including links in ancestor components. The configured root itself
  is an explicit mount and may be a link; its canonical target becomes the boundary.

On POSIX systems, strict name checks and recovery of actual names for resource policies
enumerate parent directories. These directories need read permission as well as search
permission. Searchable-only directories are unsupported: when the real directory entry
cannot be verified, resolution fails closed instead of serving the unverified resource.

Use `SymlinkPolicy::WithinRoot` when links are intentional. With strict checking enabled,
an exactly spelled link is accepted only when its resolved target stays within the root.
An escape outside the root always returns **404 Not Found**, even when strict checking is
disabled. If compatibility requires filename aliases or in-root links, opt in explicitly
and attach a resource policy when the root contains non-public files:

```rust
use salvo::serve_static::{StaticDir, StaticResourceAuthorizer, SymlinkPolicy};

fn compatible_assets(policy: impl StaticResourceAuthorizer + 'static) -> StaticDir {
    StaticDir::new("assets")
        .strict_path_check(false)
        .symlink_policy(SymlinkPolicy::WithinRoot)
        .authorize(policy)
}
```

## Authorize resolved resources in a shared root

`StaticDir::authorize` accepts a synchronous closure or a `StaticResourceAuthorizer` for
asynchronous checks. The policy receives the request, read-only `Depot` and a
`StaticResource`. Its `root()` and `path()` are canonical paths after containment checks;
`relative_path()` is the canonical target relative to that root, and is empty for the root
itself. `metadata()`, `is_dir()` and `is_file()` describe the resolved resource's metadata
snapshot. Use these paths for filesystem policy instead of comparing the raw request URL.

Authorization checks the real target, while response MIME inference and download filenames
follow the logical resource/request name, subject to explicit content-type or disposition
overrides. For example, an allowed link requested as `logo.svg` keeps that MIME type and
download name even if its canonical target has another name. A compressed sidecar likewise
keeps the logical file's name rather than exposing the sidecar's compression suffix.

For a shared `assets` root with an `assets/private` subtree, canonicalize both paths at
startup and require an authenticated depot entry for that canonical prefix:

```rust
use salvo::prelude::*;
use salvo::serve_static::{StaticDir, StaticResource};

fn shared_assets(authenticate_session: impl Handler) -> std::io::Result<Router> {
    let root = std::fs::canonicalize("assets")?;
    let private_root = std::fs::canonicalize(root.join("private"))?;
    let policy = move |_req: &Request, depot: &Depot, resource: &StaticResource| {
        !resource.path().starts_with(&private_root)
            || depot.get::<bool>("authenticated").copied().unwrap_or(false)
    };

    Ok(Router::with_path("assets/{*path}")
        .hoop(authenticate_session)
        .goal(StaticDir::new(root).authorize(policy)))
}
```

Here `authenticate_session` verifies an existing session, inserts `authenticated = true`
only for an authenticated user, and lets anonymous requests continue so public files remain
available. A missing or false depot entry denies the private subtree. The path-prefix
comparison uses filesystem components, so a sibling such as `private-public` stays public.
If `WithinRoot` is enabled, an alias targeting `private` is checked against its canonical
private target too.

A policy returning `false` produces a terminal **404 Not Found** for the selected resource.
Later roots and fallback files are not tried after a denial. Authorization happens before
directory redirects, resource headers or bodies, including GET, HEAD, range requests and
conditional requests that could otherwise return 304. Both logical files and selected
compressed sidecars are authorized, as are default files and fallback files. An authorized
directory listing filters each entry through the policy before including it; denied entries
are omitted.

For asynchronous access checks, implement `StaticResourceAuthorizer` with `async_trait`.
This example uses a mock ACL query that yields without opening a database connection;
replace it with the application's database lookup and deny access on lookup failure:

```rust
use std::path::{Path, PathBuf};

use salvo::prelude::*;
use salvo::serve_static::{StaticDir, StaticResource, StaticResourceAuthorizer};

struct AsyncPolicy {
    private_root: PathBuf,
}

#[salvo::async_trait]
impl StaticResourceAuthorizer for AsyncPolicy {
    async fn authorize(
        &self,
        _req: &Request,
        depot: &Depot,
        resource: &StaticResource,
    ) -> bool {
        if !resource.path().starts_with(&self.private_root) {
            return true;
        }
        let Ok(user_id) = depot.get::<u64>("user_id").copied() else {
            return false;
        };
        mock_acl_query(user_id, resource.path()).await
    }
}

async fn mock_acl_query(user_id: u64, _canonical_path: &Path) -> bool {
    tokio::task::yield_now().await;
    user_id == 7
}

fn async_assets() -> std::io::Result<StaticDir> {
    let root = std::fs::canonicalize("assets")?;
    let private_root = std::fs::canonicalize(root.join("private"))?;
    Ok(StaticDir::new(root).authorize(AsyncPolicy { private_root }))
}
```

Mount `async_assets()` behind a session hoop that sets `user_id` only after authentication.
The mock ACL is illustrative; a real policy must decide which resources each user may read,
including directories if listings or directory redirects are enabled.

## Keep filesystem topology trusted

Path lookup and containment checks are not atomic. The selected file is opened before
resource authorization, and the same open handle supplies its metadata and response bytes
after authorization; it is not reopened by pathname for sending. This narrows replacement
races across an asynchronous policy check, but does not make path resolution fully race
proof. `StaticResource` itself is a path-and-metadata snapshot, not a file-handle capability.

Untrusted users must not be able to replace ancestor directories, rename served paths or
create/swap symlinks and junctions during a request. Keep uploads in a separate storage area
with suitable filesystem permissions. Permission to upload file contents alone does not
establish this topology race condition. Also prevent unauthorized modification of served
file contents while requests are in flight.

Hard links and copies publish bytes under distinct paths. Canonicalization does not reveal
that a public hard link or copy originated in a private tree, and a path policy cannot infer
that relationship. Keep such entries out of public roots or authorize them explicitly.

## Directory listing budgets and filesystem permissions

Directory listings are disabled by default. When `auto_list(true)` is enabled, `StaticDir`
examines at most 10,000 entries and buffers at most 2 MiB of encoded output. Configure both
with `listing_limits(max_entries, max_bytes)`. Hidden and excluded entries, and any internal
directory enumeration required to verify link targets, count toward the scan budget so
filtering and links cannot cause an unbounded listing scan. An overflow returns **503 Service
Unavailable** for HTML, JSON, XML and plain text; it never serves a partial listing as 200.
For large collections, serve a paginated index instead. These limits apply per request.

Small-file preloading happens only after conditional and range checks. A matching
conditional GET can return 304 without reading the whole file; MIME/charset detection may
still inspect a sample of at most 1 KiB. Byte-range responses stream only the selected range.

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-serve-static)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under [Apache License, Version 2.0](LICENSE) ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).
