use std::fs::create_dir_all;
use std::path::Path;

use salvo::prelude::*;

#[handler]
async fn index(res: &mut Response) {
    res.render(Text::Html(INDEX_HTML));
}

#[handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let files = match req.files("files").await {
        Ok(files) => files,
        Err(e) => {
            let status = if matches!(e, salvo::http::ParseError::PayloadTooLarge) {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            };
            res.status_code(status);
            res.render(Text::Plain(format!("failed to parse file upload: {e}")));
            return;
        }
    };
    if let Some(files) = files {
        let mut msgs = Vec::with_capacity(files.len());
        for file in files {
            let dest = format!("temp/{}", file.name().unwrap_or("file"));
            if let Err(e) = std::fs::copy(file.path(), Path::new(&dest)) {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Text::Plain(format!("file not found in request: {e}")));
            } else {
                msgs.push(dest);
            }
        }
        res.render(Text::Plain(format!(
            "Files uploaded:\n\n{}",
            msgs.join("\n")
        )));
    } else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Text::Plain("file not found in request"));
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    create_dir_all("temp").unwrap();
    let router = Router::new().get(index).post(upload);

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Upload files</title>
    </head>
    <body>
        <h1>Upload files</h1>
        <form action="/" method="post" enctype="multipart/form-data">
            <input type="file" name="files" multiple/>
            <input type="submit" value="upload" />
        </form>
    </body>
</html>
"#;
