#![allow(missing_docs, clippy::unwrap_used)]

use std::fs;
use std::path::Path;
#[cfg(any(windows, unix))]
use std::sync::{Arc, Mutex};

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_serve_static::{StaticDir, StaticResource, SymlinkPolicy};

fn service(directory: StaticDir) -> Service {
    Service::new(Router::with_path("{**path}").goal(directory))
}

fn request(path: &str) -> Request {
    // URL parsers can normalize dot segments before the server sees them.
    let mut request = TestClient::get("http://localhost/").build();
    *request.uri_mut() = format!("http://localhost{path}").parse().unwrap();
    request
}

async fn check(service: &Service, path: &str, expected: Option<&str>) {
    let mut response = service.handle(request(path)).await;
    assert_eq!(
        response.status_code,
        Some(if expected.is_some() {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        }),
        "{path}"
    );
    if let Some(expected) = expected {
        assert_eq!(response.take_string().await.unwrap(), expected, "{path}");
    }
}

#[tokio::test]
async fn default_policy_serves_exact_public_paths() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("PublicFiles")).unwrap();
    fs::write(root.path().join("PublicFiles/Readme.txt"), "public content").unwrap();
    let service = service(StaticDir::new(root.path().to_path_buf()));

    check(&service, "/PublicFiles/Readme.txt", Some("public content")).await;
    check(
        &service,
        "/%50ublicFiles/Readme.txt",
        Some("public content"),
    )
    .await;
}

#[tokio::test]
async fn relative_root_excludes_apply_to_defaults_sidecars_and_listing_entries() {
    let root = tempfile::Builder::new()
        .prefix("salvo-relative-excludes-")
        .tempdir_in(".")
        .unwrap();
    // Keep the configured path relative, including its explicit dot prefix,
    // without changing the process-wide working directory.
    let configured = Path::new(".").join(root.path().file_name().unwrap());
    assert!(configured.is_relative());
    fs::create_dir(root.path().join("default")).unwrap();
    fs::write(root.path().join("default/index.html"), "excluded default").unwrap();
    fs::write(root.path().join("asset.txt"), "public content").unwrap();
    fs::write(
        root.path().join("asset.txt.gz"),
        include_bytes!("../test/static/test1.txt.gz"),
    )
    .unwrap();
    fs::create_dir(root.path().join("listing")).unwrap();
    fs::write(root.path().join("listing/visible.txt"), "public content").unwrap();
    fs::write(
        root.path().join("listing/secret.txt"),
        "excluded listing entry",
    )
    .unwrap();

    let excluded_default =
        path_slash::PathBufExt::to_slash_lossy(&configured.join("default/index.html")).into_owned();
    let defaults = service(
        StaticDir::new(configured.clone())
            .defaults("index.html")
            .exclude(move |path| path == excluded_default),
    );
    check(&defaults, "/default/", None).await;

    let excluded_sidecar =
        path_slash::PathBufExt::to_slash_lossy(&configured.join("asset.txt.gz")).into_owned();
    let sidecars =
        service(StaticDir::new(configured.clone()).exclude(move |path| path == excluded_sidecar));
    check(&sidecars, "/asset.txt", Some("public content")).await;
    let mut encoded_request = request("/asset.txt");
    encoded_request
        .headers_mut()
        .insert("accept-encoding", "gzip, identity;q=0".parse().unwrap());
    let encoded_response = sidecars.handle(encoded_request).await;
    assert_eq!(
        encoded_response.status_code,
        Some(StatusCode::NOT_ACCEPTABLE)
    );
    assert!(!encoded_response.headers().contains_key("content-encoding"));

    let excluded_entry =
        path_slash::PathBufExt::to_slash_lossy(&configured.join("listing/secret.txt")).into_owned();
    let listing = service(
        StaticDir::new(configured)
            .auto_list(true)
            .exclude(move |path| path == excluded_entry),
    );
    let mut listing_request = request("/listing/");
    listing_request
        .headers_mut()
        .insert("accept", "application/json".parse().unwrap());
    let mut response = listing.handle(listing_request).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let entries: serde_json::Value = response.take_json().await.unwrap();
    let files = entries["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "visible.txt");
    check(&listing, "/listing/secret.txt", None).await;
}

#[cfg(windows)]
#[tokio::test]
async fn default_strict_spelling_rejects_windows_aliases() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("PrivateNotes")).unwrap();
    fs::write(
        root.path().join("PrivateNotes/Report.txt"),
        "private content",
    )
    .unwrap();
    fs::write(root.path().join("Public.txt"), "public content").unwrap();
    let service = service(StaticDir::new(root.path().to_path_buf()));

    for path in [
        "/privatenotes/Report.txt",
        "/PrivateNotes/report.txt",
        "/PrivateNotes./Report.txt",
        "/PrivateNotes%2e/Report.txt",
        "/PrivateNotes%2E/Report.txt",
        "/PrivateNotes/Report.txt.",
        "/PrivateNotes/Report.txt%2e",
    ] {
        check(&service, path, None).await;
    }
    check(&service, "/Public.txt", Some("public content")).await;
}

#[cfg(windows)]
#[tokio::test]
async fn relaxed_spelling_authorizes_canonical_resources() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("PublicNotes")).unwrap();
    fs::create_dir(root.path().join("PrivateNotes")).unwrap();
    fs::write(root.path().join("PublicNotes/Report.txt"), "public content").unwrap();
    fs::write(
        root.path().join("PrivateNotes/Report.txt"),
        "private content",
    )
    .unwrap();
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let service = service(
        StaticDir::new(root.path().to_path_buf())
            .strict_path_check(false)
            .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
                assert_eq!(resource.root(), canonical_root);
                assert_eq!(
                    resource.path(),
                    canonical_root.join(resource.relative_path())
                );
                assert!(resource.is_file());
                assert!(!resource.is_dir());
                assert_eq!(
                    resource.metadata().len(),
                    if resource.relative_path().starts_with("PrivateNotes") {
                        15
                    } else {
                        14
                    }
                );
                captured
                    .lock()
                    .unwrap()
                    .push(resource.relative_path().to_path_buf());
                !resource.relative_path().starts_with("PrivateNotes")
            }),
    );

    check(&service, "/publicnotes/report.TXT", Some("public content")).await;
    check(&service, "/privatenotes/report.TXT", None).await;
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Path::new("PublicNotes/Report.txt").to_path_buf(),
            Path::new("PrivateNotes/Report.txt").to_path_buf(),
        ]
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn case_insensitive_linux_mounts_use_actual_entry_names() {
    use std::process::Command;

    let metadata = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let target = Path::new(metadata["target_directory"].as_str().unwrap());
    let root = tempfile::Builder::new()
        .prefix("salvo-case-policy-")
        .tempdir_in(target)
        .unwrap();
    fs::create_dir(root.path().join("PublicNotes")).unwrap();
    fs::create_dir(root.path().join("PrivateNotes")).unwrap();
    fs::write(
        root.path().join("PublicNotes/ActualCase.txt"),
        "public content",
    )
    .unwrap();
    fs::write(
        root.path().join("PrivateNotes/ActualCase.txt"),
        "private content",
    )
    .unwrap();
    if !root.path().join("privatenotes/actualcase.txt").is_file() {
        eprintln!("SKIP: the workspace target directory is on a case-sensitive filesystem");
        return;
    }

    let strict = service(StaticDir::new(root.path().to_path_buf()));
    check(
        &strict,
        "/PublicNotes/ActualCase.txt",
        Some("public content"),
    )
    .await;
    check(&strict, "/publicnotes/actualcase.txt", None).await;
    check(&strict, "/privatenotes/actualcase.txt", None).await;

    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let relaxed = service(
        StaticDir::new(root.path().to_path_buf())
            .strict_path_check(false)
            .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
                assert_eq!(resource.root(), canonical_root);
                assert!(resource.is_file());
                captured
                    .lock()
                    .unwrap()
                    .push(resource.relative_path().to_path_buf());
                !resource.relative_path().starts_with("PrivateNotes")
            }),
    );
    check(
        &relaxed,
        "/publicnotes/actualcase.txt",
        Some("public content"),
    )
    .await;
    check(&relaxed, "/privatenotes/actualcase.txt", None).await;
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Path::new("PublicNotes/ActualCase.txt").to_path_buf(),
            Path::new("PrivateNotes/ActualCase.txt").to_path_buf(),
        ]
    );

    // Even an exactly spelled public link can point at an implicit alias on
    // this mount. Authorization must receive the target's actual entry names.
    for (target, name) in [
        ("publicnotes/actualcase.txt", "public-link.txt"),
        ("privatenotes/actualcase.txt", "private-link.txt"),
    ] {
        match std::os::unix::fs::symlink(target, root.path().join(name)) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
                ) =>
            {
                eprintln!(
                    "SKIP: direct case-alias checks passed; this filesystem does not permit the additional Unix symlink-target checks: {error}"
                );
                return;
            }
            Err(error) => panic!("could not create wrong-case target link {name}: {error}"),
        }
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let linked = service(
        StaticDir::new(root.path().to_path_buf())
            .symlink_policy(SymlinkPolicy::WithinRoot)
            .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
                assert!(resource.is_file());
                captured
                    .lock()
                    .unwrap()
                    .push(resource.relative_path().to_path_buf());
                !resource.relative_path().starts_with("PrivateNotes")
            }),
    );
    check(&linked, "/public-link.txt", Some("public content")).await;
    check(&linked, "/private-link.txt", None).await;
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Path::new("PublicNotes/ActualCase.txt").to_path_buf(),
            Path::new("PrivateNotes/ActualCase.txt").to_path_buf(),
        ]
    );
}

#[cfg(unix)]
fn directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn directory_link(target: &Path, link: &Path) {
    use std::process::Command;

    fn quoted_path(path: &Path) -> String {
        // PowerShell's filesystem provider accepts ordinary absolute paths.
        // Convert canonical verbatim prefixes before passing its literal paths.
        let path = path.to_str().unwrap();
        let path = if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{path}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
        };
        format!("'{}'", path.replace('\'', "''"))
    }

    let target = fs::canonicalize(target).unwrap();
    let link = fs::canonicalize(link.parent().unwrap())
        .unwrap()
        .join(link.file_name().unwrap());
    assert!(target.is_absolute());
    assert!(link.is_absolute());
    let script = format!(
        "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
        quoted_path(&link),
        quoted_path(&target)
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "could not create junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn default_policy_rejects_ancestor_and_final_directory_links() {
    assert_eq!(SymlinkPolicy::default(), SymlinkPolicy::Deny);
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("real")).unwrap();
    fs::write(root.path().join("real/public.txt"), "public content").unwrap();
    directory_link(&root.path().join("real"), &root.path().join("linked"));
    let service = service(StaticDir::new(root.path().to_path_buf()).auto_list(true));

    for path in ["/linked", "/linked/", "/linked/public.txt"] {
        check(&service, path, None).await;
    }
    check(&service, "/real/public.txt", Some("public content")).await;
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn within_root_links_allow_exact_names_but_reject_outside_targets() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("real")).unwrap();
    fs::write(root.path().join("real/public.txt"), "public content").unwrap();
    fs::write(outside.path().join("private.txt"), "private content").unwrap();
    directory_link(&root.path().join("real"), &root.path().join("inside-link"));
    directory_link(outside.path(), &root.path().join("outside-link"));
    let service = service(
        StaticDir::new(root.path().to_path_buf())
            .auto_list(true)
            .symlink_policy(SymlinkPolicy::WithinRoot),
    );

    check(&service, "/inside-link/public.txt", Some("public content")).await;
    for path in [
        "/outside-link",
        "/outside-link/",
        "/outside-link/private.txt",
    ] {
        check(&service, path, None).await;
    }
    #[cfg(windows)]
    for path in ["/INSIDE-LINK/public.txt", "/inside-link%2e/public.txt"] {
        check(&service, path, None).await;
    }
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn directory_alias_listing_excludes_use_the_logical_alias_path() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("real")).unwrap();
    fs::write(root.path().join("real/visible.txt"), "public content").unwrap();
    fs::write(root.path().join("real/secret.txt"), "private content").unwrap();
    directory_link(&root.path().join("real"), &root.path().join("alias"));
    let excluded =
        path_slash::PathBufExt::to_slash_lossy(&root.path().join("alias/secret.txt")).into_owned();
    let service = service(
        StaticDir::new(root.path().to_path_buf())
            .auto_list(true)
            .symlink_policy(SymlinkPolicy::WithinRoot)
            .exclude(move |path| path == excluded),
    );

    let mut listing_request = request("/alias/");
    listing_request
        .headers_mut()
        .insert("accept", "application/json".parse().unwrap());
    let mut response = service.handle(listing_request).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let entries: serde_json::Value = response.take_json().await.unwrap();
    let files = entries["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "visible.txt");
    check(&service, "/alias/secret.txt", None).await;
    check(&service, "/real/secret.txt", Some("private content")).await;
}

#[cfg(unix)]
#[tokio::test]
async fn listing_entry_budget_includes_link_target_name_scans() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    for index in 0..50 {
        let target = format!("file-{index:03}.txt");
        fs::write(root.path().join(&target), "public content").unwrap();
        symlink(&target, root.path().join(format!("link-{index:03}.txt"))).unwrap();
    }
    let denied = service(
        StaticDir::new(root.path().to_path_buf())
            .auto_list(true)
            .listing_limits(150, 64 * 1024),
    );
    let allowed = service(
        StaticDir::new(root.path().to_path_buf())
            .auto_list(true)
            .symlink_policy(SymlinkPolicy::WithinRoot)
            .listing_limits(150, 64 * 1024),
    );

    let mut denied_request = request("/");
    denied_request
        .headers_mut()
        .insert("accept", "application/json".parse().unwrap());
    let mut response = denied.handle(denied_request).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let entries: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(entries["files"].as_array().unwrap().len(), 50);

    // The 100 top-level entries fit. Following 50 distinct targets also scans
    // their parent for exact names; those internal entries share the budget.
    let mut allowed_request = request("/");
    allowed_request
        .headers_mut()
        .insert("accept", "application/json".parse().unwrap());
    let response = allowed.handle(allowed_request).await;
    assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn configured_root_link_is_an_explicit_mount() {
    let target = tempfile::tempdir().unwrap();
    let mount = tempfile::tempdir().unwrap();
    fs::write(target.path().join("public.txt"), "public content").unwrap();
    let link = mount.path().join("static-mount");
    directory_link(target.path(), &link);
    let canonical_root = fs::canonicalize(target.path()).unwrap();
    let service = service(StaticDir::new(link).authorize(
        move |_: &Request, _: &Depot, resource: &StaticResource| {
            assert_eq!(resource.root(), canonical_root);
            assert_eq!(resource.path(), canonical_root.join("public.txt"));
            assert_eq!(resource.relative_path(), Path::new("public.txt"));
            true
        },
    ));

    check(&service, "/public.txt", Some("public content")).await;
}

#[cfg(unix)]
#[tokio::test]
async fn final_file_symlinks_obey_policy_and_canonical_authorization() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("private")).unwrap();
    fs::write(root.path().join("public.txt"), "public content").unwrap();
    fs::write(root.path().join("private/secret.txt"), "private content").unwrap();
    fs::write(outside.path().join("outside.txt"), "outside content").unwrap();
    symlink(
        root.path().join("public.txt"),
        root.path().join("public-link.txt"),
    )
    .unwrap();
    symlink(
        root.path().join("private/secret.txt"),
        root.path().join("private-link.txt"),
    )
    .unwrap();
    symlink(
        outside.path().join("outside.txt"),
        root.path().join("outside-link.txt"),
    )
    .unwrap();
    let denied = service(StaticDir::new(root.path().to_path_buf()));
    let allowed = service(
        StaticDir::new(root.path().to_path_buf()).symlink_policy(SymlinkPolicy::WithinRoot),
    );
    let authorized = service(
        StaticDir::new(root.path().to_path_buf())
            .symlink_policy(SymlinkPolicy::WithinRoot)
            .authorize(|_: &Request, _: &Depot, resource: &StaticResource| {
                !resource.relative_path().starts_with("private")
            }),
    );

    check(&denied, "/public-link.txt", None).await;
    check(&denied, "/private-link.txt", None).await;
    check(&allowed, "/public-link.txt", Some("public content")).await;
    check(&allowed, "/private-link.txt", Some("private content")).await;
    check(&allowed, "/outside-link.txt", None).await;
    check(&authorized, "/private-link.txt", None).await;
}

#[cfg(unix)]
#[tokio::test]
async fn allowed_file_links_preserve_logical_metadata_and_compressed_sidecars() {
    use std::io::Write;
    use std::os::unix::fs::symlink;

    use flate2::Compression;
    use flate2::write::GzEncoder;

    let root = tempfile::tempdir().unwrap();
    let css = b"body { color: #123456; }\n";
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(css).unwrap();
    let compressed = encoder.finish().unwrap();
    fs::write(root.path().join("payload.bin"), css).unwrap();
    fs::write(root.path().join("compressed.bin"), &compressed).unwrap();
    symlink("payload.bin", root.path().join("asset.css")).unwrap();
    symlink("compressed.bin", root.path().join("asset.css.gz")).unwrap();
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let service = service(
        StaticDir::new(root.path().to_path_buf())
            .symlink_policy(SymlinkPolicy::WithinRoot)
            .disposition_type("attachment")
            .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
                assert_eq!(resource.root(), canonical_root);
                assert!(resource.is_file());
                assert_eq!(
                    resource.path(),
                    canonical_root.join(resource.relative_path())
                );
                captured
                    .lock()
                    .unwrap()
                    .push(resource.relative_path().to_path_buf());
                true
            }),
    );

    for encoding in ["identity", "gzip, identity;q=0"] {
        let mut request = request("/asset.css");
        request
            .headers_mut()
            .insert("accept-encoding", encoding.parse().unwrap());
        let mut response = service.handle(request).await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{encoding}");
        assert_eq!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .split(';')
                .next(),
            Some("text/css"),
            "{encoding}"
        );
        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=\"asset.css\"",
            "{encoding}"
        );
        if encoding == "identity" {
            assert!(!response.headers().contains_key("content-encoding"));
            assert_eq!(response.take_bytes(None).await.unwrap().as_ref(), css);
        } else {
            assert_eq!(response.headers()["content-encoding"], "gzip");
            assert_eq!(
                response.take_bytes(None).await.unwrap().as_ref(),
                compressed
            );
        }
    }
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Path::new("payload.bin").to_path_buf(),
            Path::new("payload.bin").to_path_buf(),
            Path::new("compressed.bin").to_path_buf(),
        ]
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_short_names_require_relaxed_spelling_and_canonical_authorization() {
    use std::ffi::OsString;
    use std::process::Command;

    fn short_directory_name(path: &Path) -> Option<OsString> {
        // The command is constant: the generated fixture path is passed only
        // as the process working directory, never interpolated as shell code.
        let output = Command::new("cmd.exe")
            .args(["/D", "/C", "for %I in (.) do @echo %~fsI"])
            .current_dir(path)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let output = String::from_utf8_lossy(&output.stdout);
        let short = Path::new(output.trim());
        let name = short.file_name()?;
        (name != path.file_name()?
            && fs::canonicalize(short).ok()? == fs::canonicalize(path).ok()?)
        .then(|| name.to_os_string())
    }

    let root = tempfile::tempdir().unwrap();
    let public = root.path().join("PublicDocumentArchive");
    let private = root.path().join("PrivateDocumentArchive");
    fs::create_dir(&public).unwrap();
    fs::create_dir(&private).unwrap();
    fs::write(public.join("report.txt"), "public content").unwrap();
    fs::write(private.join("report.txt"), "private content").unwrap();
    let (Some(public_alias), Some(private_alias)) = (
        short_directory_name(&public),
        short_directory_name(&private),
    ) else {
        eprintln!("SKIP: this temporary directory volume does not provide distinct 8.3 aliases");
        return;
    };
    let public_alias = format!("/{}/report.txt", public_alias.to_str().unwrap());
    let private_alias = format!("/{}/report.txt", private_alias.to_str().unwrap());
    let strict = service(StaticDir::new(root.path().to_path_buf()));
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let relaxed = service(
        StaticDir::new(root.path().to_path_buf())
            .strict_path_check(false)
            .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
                assert_eq!(resource.root(), canonical_root);
                assert_eq!(
                    resource.path(),
                    canonical_root.join(resource.relative_path())
                );
                assert!(resource.is_file());
                !resource
                    .relative_path()
                    .starts_with("PrivateDocumentArchive")
            }),
    );

    check(&strict, &public_alias, None).await;
    check(&strict, &private_alias, None).await;
    check(
        &strict,
        "/PublicDocumentArchive/report.txt",
        Some("public content"),
    )
    .await;
    check(&relaxed, &public_alias, Some("public content")).await;
    check(&relaxed, &private_alias, None).await;
}
