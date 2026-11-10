use std::sync::OnceLock;
use std::time::Duration;

use salvo_core::http::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use salvo_core::http::{FormLimits, HeaderValue, Method, ParseError, StatusCode};
use salvo_core::test::{RequestBuilder, TestClient};
use salvo_core::{Request, Response, Router, Service, handler};
use salvo_jwt_auth::oidc::CachePolicy;
use serde_json::json;

const MAX_INPUT: usize = 8192;
fn run(future: impl Future<Output = ()>) {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("fuzz runtime")
        })
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), future)
                .await
                .expect("bounded property timed out");
        });
}

pub fn cache_control(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    let mid = data.len() / 2;
    if let (Ok(a), Ok(b)) = (
        HeaderValue::from_bytes(&data[..mid]),
        HeaderValue::from_bytes(&data[mid..]),
    ) {
        let mut joined = a.as_bytes().to_vec();
        joined.push(b',');
        joined.extend_from_slice(b.as_bytes());
        let combined = HeaderValue::from_bytes(&joined).unwrap();
        let policy = CachePolicy::from_header_values([&a, &b]);
        assert_eq!(policy, CachePolicy::from_header_val(Some(&combined)));
        for age in [
            Some(policy.max_age),
            policy.stale_while_revalidate,
            policy.stale_if_error,
        ]
        .into_iter()
        .flatten()
        {
            assert!(age <= Duration::from_secs(30 * 86400));
        }
    }
    let opaque = data
        .iter()
        .take(128)
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let control = HeaderValue::from_str(&format!(
        "max-age=7, extension=\"{opaque},max-age=2592000,ignored\", stale-if-error=0"
    ))
    .unwrap();
    let policy = CachePolicy::from_header_val(Some(&control));
    assert_eq!(policy.max_age, Duration::from_secs(7));
    assert_eq!(policy.stale_if_error, Some(Duration::ZERO));
}

fn roundtrip(document: salvo_oapi::OpenApi) {
    let value = serde_json::to_value(document).unwrap();
    let decoded: salvo_oapi::OpenApi =
        serde_json::from_value(value.clone()).expect("serialized document must deserialize");
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}
pub fn openapi_roundtrip(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    if let Ok(doc) = serde_json::from_slice::<salvo_oapi::OpenApi>(data) {
        roundtrip(doc);
    }
    let text = String::from_utf8_lossy(&data[..data.len().min(256)]);
    let example = json!({"value":text,"length":data.len()});
    let source = json!({"openapi":"3.1.0","info":{"title":text,"version":"1"},
        "paths":{"/items":{"post":{"responses":{"200":{"description":text,"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Item"},"example":example}}}}}}},
        "components":{"schemas":{"Item":{"type":"object","description":text,"properties":{"value":{"type":"string","examples":[text]}}}}},"x-fuzz":example});
    let doc: salvo_oapi::OpenApi = serde_json::from_value(source.clone()).unwrap();
    let value = serde_json::to_value(&doc).unwrap();
    assert_eq!(value["info"]["title"], source["info"]["title"]);
    assert_eq!(
        value["paths"]["/items"]["post"]["responses"]["200"]["content"],
        source["paths"]["/items"]["post"]["responses"]["200"]["content"]
    );
    assert_eq!(value["components"], source["components"]);
    assert_eq!(value["x-fuzz"], example);
    roundtrip(doc);
}

const DATE: &str = "Wed, 21 Oct 2015 07:28:00 GMT";
#[handler]
async fn representation(req: &mut Request, res: &mut Response) {
    res.status_code(StatusCode::from_u16(req.query::<u16>("status").unwrap()).unwrap());
    res.add_header(ETAG, "\"current,tag\"", true).unwrap();
    res.add_header(LAST_MODIFIED, DATE, true).unwrap();
    res.render("representation");
}
pub fn conditional_headers(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    run(async {
        let flags = data.first().copied().unwrap_or(0);
        let method = [Method::GET, Method::HEAD, Method::POST, Method::DELETE]
            [(flags as usize >> 2) % 4]
            .clone();
        let status = [200, 201, 404, 500][(flags as usize >> 4) % 4];
        let condition = match flags % 4 {
            0 => None,
            1 => Some("\"other\""),
            2 => Some("W/\"current,tag\""),
            _ => Some("*"),
        };
        let matching_date = flags & 64 != 0;
        let service = Service::new(
            Router::with_hoop(salvo_extra::caching_headers::CachingHeaders::new())
                .goal(representation),
        );
        let mut builder =
            RequestBuilder::new(format!("http://localhost/?status={status}"), method.clone());
        if let Some(condition) = condition {
            builder = builder.add_header(IF_NONE_MATCH, condition, true);
        }
        if matching_date {
            builder = builder.add_header(IF_MODIFIED_SINCE, DATE, true);
        }
        if let Ok(value) = HeaderValue::from_bytes(data.get(1..).unwrap_or_default()) {
            builder = builder.add_header(ETAG, value, true);
        }
        let response = builder.send(&service).await;
        let unchanged = status == 200
            && matches!(method, Method::GET | Method::HEAD)
            && match condition {
                None => matching_date,
                Some("\"other\"") => false,
                Some(_) => true,
            };
        assert_eq!(
            response.status_code,
            Some(if unchanged {
                StatusCode::NOT_MODIFIED
            } else {
                StatusCode::from_u16(status).unwrap()
            })
        );
        assert_eq!(response.headers()[ETAG], "\"current,tag\"");
        if unchanged {
            assert!(response.body.is_none());
        }
    });
}

pub fn multipart(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    run(async {
        let count = data.first().copied().unwrap_or(0) as usize % 9;
        let max_parts = data.get(1).copied().unwrap_or(8) as usize % 9;
        let max_files = data.get(2).copied().unwrap_or(8) as usize % 9;
        let mut files = 0;
        let mut body = String::new();
        let value = crate::non_empty_token(data.get(3..).unwrap_or_default(), "x", 32);
        for i in 0..count {
            let file = data.get(i + 3).copied().unwrap_or(0) & 1 != 0;
            files += usize::from(file);
            let filename = if file { "; filename=\"f.txt\"" } else { "" };
            body.push_str(&format!("--fuzz-boundary\r\nContent-Disposition: form-data; name=\"f\"{filename}\r\n\r\n{value}\r\n"));
        }
        body.push_str("--fuzz-boundary--\r\n");
        let limits = FormLimits::new().max_parts(max_parts).max_files(max_files);
        let mut req = TestClient::post("http://localhost/")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=fuzz-boundary",
                true,
            )
            .body(body)
            .build();
        req.set_form_limits(limits);
        let result = req.form_data().await;
        if count > max_parts || files > max_files {
            assert!(matches!(result, Err(ParseError::PayloadTooLarge)));
        } else {
            let form = result.expect("generated multipart must parse");
            assert_eq!(form.fields.get_vec("f").map_or(0, Vec::len), count - files);
            assert_eq!(form.files.get_vec("f").map_or(0, Vec::len), files);
            if let Some(fields) = form.fields.get_vec("f") {
                for field in fields {
                    assert_eq!(field, &value);
                }
            }
            if let Some(files) = form.files.get_vec("f") {
                for file in files {
                    assert_eq!(std::fs::read(file.path()).unwrap(), value.as_bytes());
                }
            }
        }
        let mut req = TestClient::post("http://localhost/")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=fuzz-boundary",
                true,
            )
            .body(data.to_vec())
            .build();
        req.set_form_limits(limits);
        if let Ok(form) = req.form_data().await {
            let fields: usize = form.fields.iter_all().map(|(_, v)| v.len()).sum();
            let files: usize = form.files.iter_all().map(|(_, v)| v.len()).sum();
            assert!(fields + files <= max_parts);
            assert!(files <= max_files);
        }
    });
}

async fn head_offset(service: &Service, expected: usize, length: usize) {
    use salvo_tus::*;
    let response = TestClient::head("http://localhost/uploads/fuzz-upload")
        .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers()[H_UPLOAD_OFFSET], expected.to_string());
    assert_eq!(response.headers()[H_UPLOAD_LENGTH], length.to_string());
}
pub fn tus_state(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    run(async {
        use salvo_tus::*;
        let root = tempfile::tempdir().unwrap();
        let length = data.first().copied().unwrap_or(0) as usize % 33;
        let service = Service::new(
            Tus::new()
                .path("/uploads")
                .storage_root(root.path())
                .upload_id_naming_function(|_, _| async { Ok("fuzz-upload".to_owned()) })
                .into_router(),
        );
        let response = TestClient::post("http://localhost/uploads")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .add_header(H_UPLOAD_LENGTH, length.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::CREATED));
        let mut contents = Vec::new();
        head_offset(&service, 0, length).await;
        for &op in data.get(1..).unwrap_or_default().iter().take(16) {
            let remaining = length - contents.len();
            let mode = op % 4;
            let amount = if mode == 2 {
                remaining + 1
            } else {
                (op as usize / 4) % (remaining + 1)
            };
            let bytes = vec![op; amount];
            let offset = contents.len() + usize::from(mode == 1);
            let response = TestClient::patch("http://localhost/uploads/fuzz-upload")
                .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
                .add_header(
                    H_CONTENT_TYPE,
                    if mode == 3 {
                        "text/plain"
                    } else {
                        CT_OFFSET_OCTET_STREAM
                    },
                    true,
                )
                .add_header(H_UPLOAD_OFFSET, offset.to_string(), true)
                .add_header(H_CONTENT_LENGTH, amount.to_string(), true)
                .body(bytes.clone())
                .send(&service)
                .await;
            assert_eq!(
                response.status_code,
                Some(match mode {
                    1 => StatusCode::CONFLICT,
                    2 => StatusCode::PAYLOAD_TOO_LARGE,
                    3 => StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    _ => StatusCode::NO_CONTENT,
                })
            );
            if mode == 0 {
                contents.extend(bytes);
            }
            head_offset(&service, contents.len(), length).await;
            assert_eq!(
                std::fs::read(root.path().join("data/fuzz-upload.bin")).unwrap(),
                contents
            );
        }
        let bytes = vec![42; length - contents.len()];
        let response = TestClient::patch("http://localhost/uploads/fuzz-upload")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .add_header(H_CONTENT_TYPE, CT_OFFSET_OCTET_STREAM, true)
            .add_header(H_UPLOAD_OFFSET, contents.len().to_string(), true)
            .add_header(H_CONTENT_LENGTH, bytes.len().to_string(), true)
            .body(bytes.clone())
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
        contents.extend(bytes);
        head_offset(&service, length, length).await;
        assert_eq!(
            std::fs::read(root.path().join("data/fuzz-upload.bin")).unwrap(),
            contents
        );
        let response = TestClient::delete("http://localhost/uploads/fuzz-upload")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
        let response = TestClient::head("http://localhost/uploads/fuzz-upload")
            .add_header(H_TUS_RESUMABLE, TUS_VERSION, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
        assert!(!root.path().join("data/fuzz-upload.bin").exists());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    const TARGETS: [(&str, fn(&[u8])); 5] = [
        ("cache_control", cache_control),
        ("openapi_roundtrip", openapi_roundtrip),
        ("conditional_headers", conditional_headers),
        ("multipart", multipart),
        ("tus_state", tus_state),
    ];
    #[test]
    fn checked_in_corpus_replays_the_same_properties() {
        for (name, check) in TARGETS {
            for entry in
                std::fs::read_dir(format!("{}/corpus/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
            {
                let path = entry.unwrap().path();
                eprintln!("replaying {}", path.display());
                check(&std::fs::read(path).unwrap());
            }
            check(&[]);
        }
    }
    #[test]
    fn deterministic_model_cases_cover_every_conditional_flag_and_upload_mode() {
        for flag in 0..=255 {
            conditional_headers(&[flag]);
        }
        for flag in 0u8..=255 {
            let data = [
                flag,
                flag.wrapping_add(1),
                flag.wrapping_add(2),
                0,
                4,
                1,
                2,
                3,
                8,
                12,
            ];
            cache_control(&data);
            openapi_roundtrip(&data);
            multipart(&data);
            tus_state(&data);
        }
    }
}
