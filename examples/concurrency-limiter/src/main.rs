use std::fs::create_dir_all;
use std::path::Path;

use salvo::prelude::*;

// Handler for serving the index page with upload forms
#[handler]
async fn index(res: &mut Response) {
    res.render(Text::Html(INDEX_HTML));
}

// Handler for processing file uploads with a simulated delay
#[handler]
async fn upload(req: &mut Request, res: &mut Response) {
    // Extract file from the multipart form data
    let file = req.file("file").await;
    // Simulate a long-running operation (10 seconds)
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    if let Some(file) = file {
        // Generate destination path for the uploaded file
        let dest = format!("temp/{}", file.name().unwrap_or("file"));
        tracing::debug!(dest = %dest, "upload file");

        // Copy file to destination
        if let Err(e) = std::fs::copy(file.path(), Path::new(&dest)) {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Text::Plain(format!("file not found in request: {e}")));
        } else {
            res.render(Text::Plain(format!("File uploaded to {dest}")));
        }
    } else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Text::Plain("file not found in request"));
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Create temporary directory for file uploads
    create_dir_all("temp").unwrap();

    // Configure router with two upload endpoints:
    // - /limited: Only allows one concurrent upload (with concurrency limiter)
    // - /unlimit: Allows unlimited concurrent uploads
    let router = Router::new()
        .get(index)
        .push(
            Router::new()
                .hoop(max_concurrency(1)) // Limit concurrent requests to 1
                .path("limited")
                .post(upload),
        )
        .push(Router::with_path("unlimit").post(upload));

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

// HTML template for the upload forms page
static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Upload file</title>
    </head>
    <body>
        <h1>Upload file</h1>
        <form action="/unlimit" method="post" enctype="multipart/form-data">
            <h3>Unlimit</h3>
            <input type="file" name="file" />
            <input type="submit" value="upload" />
        </form>
        <form action="/limited" method="post" enctype="multipart/form-data">
            <h3>Limited</h3>
            <input type="file" name="file" />
            <input type="submit" value="upload" />
        </form>
    </body>
</html>
"#;
