use std::fs::create_dir_all;
use std::path::Path;

use salvo::prelude::*;

#[fn_handler]
async fn index(res: &mut Response) {
    res.render_html_text(INDEX_HTML);
}
#[fn_handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.get_file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.file_name().unwrap_or_else(|| "file".into()));
        println!("{}", dest);
        if let Err(e) = std::fs::copy(&file.path(), Path::new(&dest)) {
            res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render_plain_text(&format!("file not found in request: {}", e.to_string()));
        } else {
            res.render_plain_text(&format!("File uploaded to {}", dest));
        }
    } else {
        res.set_status_code(StatusCode::BAD_REQUEST);
        res.render_plain_text("file not found in request");
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    create_dir_all("temp").unwrap();
    let router = Router::new().get(index).post(upload);
    Server::bind(&"127.0.0.1:7878".parse().unwrap())
        .serve(Service::new(router))
        .await
        .unwrap();
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Upload file</title>
    </head>
    <body>
        <h1>Upload file</h1>
        <form action="/" method="post" enctype="multipart/form-data">
            <input type="file" name="file" />
            <input type="submit" value="upload" />
        </form>
    </body>
</html>
"#;
