#![allow(missing_docs, clippy::unwrap_used)]

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_serve_static::StaticDir;

fn service(directory: StaticDir) -> Service {
    Service::new(Router::with_path("{**path}").goal(directory))
}

async fn check_file(service: &Service, path: &str, expected: Option<&str>) {
    let mut response = TestClient::get(format!("http://localhost/{path}"))
        .send(service)
        .await;
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
async fn dot_files_in_default_and_fallback_paths_respect_opt_in() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".static");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".env"), "secret").unwrap();
    std::fs::write(root.join(".git/config"), "repository").unwrap();
    std::fs::write(root.join("public.txt"), "public").unwrap();

    for include in [false, true] {
        let directory = service(StaticDir::new(root.clone()).include_dot_files(include));
        check_file(&directory, "public.txt", Some("public")).await;
        check_file(&directory, ".env", include.then_some("secret")).await;
        check_file(&directory, ".git/config", include.then_some("repository")).await;

        for path in [".env", ".git/config"] {
            let expected = include.then_some(if path == ".env" {
                "secret"
            } else {
                "repository"
            });
            let defaults = service(
                StaticDir::new(root.clone())
                    .include_dot_files(include)
                    .defaults(path),
            );
            check_file(&defaults, "", expected).await;
            let fallback = service(
                StaticDir::new(root.clone())
                    .include_dot_files(include)
                    .fallback(path),
            );
            check_file(&fallback, "missing", expected).await;
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn visible_alias_to_dot_directory_respects_opt_in() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    std::fs::write(temp.path().join(".git/config"), "repository").unwrap();
    std::os::unix::fs::symlink(".git", temp.path().join("alias")).unwrap();
    for include in [false, true] {
        let directory =
            service(StaticDir::new(temp.path().to_path_buf()).include_dot_files(include));
        check_file(&directory, "alias/config", include.then_some("repository")).await;
    }
}

#[cfg(windows)]
fn short_name(path: &std::path::Path) -> String {
    use std::os::windows::process::CommandExt;

    // Ask cmd for the actual short name without adding unsafe Windows FFI.
    // Pass the fixture path through the environment rather than shell source.
    let output = std::process::Command::new("cmd")
        .args(["/d", "/c"])
        .raw_arg("for %I in (\"%SALVO_TEST_PATH%\") do @echo %~snxI")
        .env("SALVO_TEST_PATH", path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[cfg(windows)]
#[tokio::test]
async fn windows_short_names_cannot_expose_dot_files() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    std::fs::write(temp.path().join(".env"), "secret").unwrap();
    std::fs::write(temp.path().join(".git/config"), "repository").unwrap();
    let env = short_name(&temp.path().join(".env"));
    let git = short_name(&temp.path().join(".git"));
    if env == ".env" || git == ".git" {
        eprintln!("skipping 8.3 alias requests: the test volume did not create short names");
        return;
    }
    assert!(!env.starts_with('.'));
    assert!(!git.starts_with('.'));
    for include in [false, true] {
        let directory =
            service(StaticDir::new(temp.path().to_path_buf()).include_dot_files(include));
        check_file(&directory, &env, include.then_some("secret")).await;
        check_file(
            &directory,
            &format!("{git}/config"),
            include.then_some("repository"),
        )
        .await;
    }
}
