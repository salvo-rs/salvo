use std::fs::create_dir_all;
use std::path::Path;

use salvo::prelude::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn index(res: &mut Response) {
    res.render_html_text(INDEX_HTML);
}
#[fn_handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.get_file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.filename().unwrap_or_else(|| "file".into()));
        if let Err(e) = std::fs::copy(&file.path, Path::new(&dest)) {
            res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render_plain_text(&format!("file not found in request: {}", e.to_string()));
        } else {
            res.render_plain_text("Ok");
        }
    } else {
        res.set_status_code(StatusCode::BAD_REQUEST);
        res.render_plain_text("file not found in request");
    }
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "hello_world=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    create_dir_all("temp").unwrap();
    let router = Router::new().get(index).post(upload);
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Upload</title>
    </head>
    <body>
        <h1>Upload</h1>
        <form action="/" method="post" enctype="multipart/form-data">
        <input type="file" name="file" />
        <input type="submit" />
        </form>
    </body>
</html>
"#;
