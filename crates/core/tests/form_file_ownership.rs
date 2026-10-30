//! Owned file extractors must retain their temporary uploads.

#![cfg(feature = "test")]

use salvo_core::extract::{Extractible, FormFile, FormFiles};
use salvo_core::test::TestClient;
use salvo_core::{Depot, Request};

fn request() -> Request {
    TestClient::post("http://localhost/")
        .add_header("content-type", "multipart/form-data; boundary=b", true)
        .body(concat!(
            "--b\r\nContent-Disposition: form-data; name=\"files\"; filename=\"a.txt\"\r\n",
            "Content-Type: text/plain\r\n\r\nfirst\r\n",
            "--b\r\nContent-Disposition: form-data; name=\"files\"; filename=\"b.txt\"\r\n",
            "\r\nsecond\r\n--b--\r\n"
        ))
        .build()
}

#[test]
fn owned_form_file_survives_request_and_other_clones() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (req, mut file) = runtime.block_on(async {
        let mut req = request();
        let file = FormFile::extract_with_arg(&mut req, &mut Depot::new(), "files")
            .await
            .unwrap();
        (req, file)
    });
    let path = file.path().clone();
    let directory = path.parent().unwrap().to_owned();
    assert_eq!(file.name(), Some("a.txt"));
    assert_eq!(file.size(), 5);
    assert_eq!(file.content_type().unwrap(), "text/plain");
    *file.name_mut().unwrap() = "changed.txt".into();
    file.headers_mut()
        .insert("x-test", "value".parse().unwrap());
    let clone = file.clone();
    // Outside the runtime, the upload's last-owner cleanup runs synchronously.
    drop(req);
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
    drop(file);
    assert_eq!(clone.name(), Some("changed.txt"));
    assert_eq!(clone.headers()["x-test"], "value");
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
    drop(clone);
    assert!(!path.exists());
    assert!(!directory.exists());
}

#[test]
fn owned_form_files_retain_each_upload_after_into_inner() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (req, files) = runtime.block_on(async {
        let mut req = request();
        let files = FormFiles::extract_with_arg(&mut req, &mut Depot::new(), "files")
            .await
            .unwrap();
        (req, files)
    });
    let paths: Vec<_> = files.iter().map(|file| file.path().clone()).collect();
    let clone = files.clone();
    drop(req);
    let inner = clone.into_inner();
    drop(files);
    for (path, expected) in paths.iter().zip([b"first".as_slice(), b"second"]) {
        assert_eq!(std::fs::read(path).unwrap(), expected);
    }
    drop(inner);
    for path in paths {
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }
}
