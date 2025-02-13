use salvo::csrf::*;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

// Handler for serving the home page with links to different CSRF protection methods
#[handler]
pub async fn home(res: &mut Response) {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Csrf CookieStore</title></head>
    <body>
    <h2>Csrf Exampe: CookieStore</h2>
    <ul>
        <li><a href="/bcrypt/">Bcrypt</a></li>
        <li><a href="/hmac/">Hmac</a></li>
        <li><a href="/aes_gcm/">Aes Gcm</a></li>
        <li><a href="/ccp/">chacha20poly1305</a></li>
    </ul>
    </body>"#;
    res.render(Text::Html(html));
}

// Handler for GET requests that displays a form with CSRF token
#[handler]
pub async fn get_page(depot: &mut Depot, res: &mut Response) {
    let new_token = depot.csrf_token().unwrap_or_default();
    res.render(Text::Html(get_page_html(new_token, "")));
}

// Handler for POST requests that processes form submission with CSRF validation
#[handler]
pub async fn post_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    // Define data structure for form submission
    #[derive(Deserialize, Serialize, Debug)]
    struct Data {
        csrf_token: String,
        message: String,
    }
    // Parse the submitted form data into the Data struct
    let data = req.parse_form::<Data>().await.unwrap();
    // Log the received form data for debugging
    tracing::info!("posted data: {:?}", data);
    // Generate a new CSRF token for the next request
    let new_token = depot.csrf_token().unwrap_or_default();
    // Generate HTML response with the new token and display the submitted data
    let html = get_page_html(new_token, &format!("{data:#?}"));
    // Send the HTML response back to the client
    res.render(Text::Html(html));
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Configure CSRF token finder in form data
    let form_finder = FormFinder::new("csrf_token");

    // Initialize different CSRF protection methods
    let bcrypt_csrf = bcrypt_cookie_csrf(form_finder.clone());
    let hmac_csrf = hmac_cookie_csrf(*b"01234567012345670123456701234567", form_finder.clone());
    let aes_gcm_cookie_csrf =
        aes_gcm_cookie_csrf(*b"01234567012345670123456701234567", form_finder.clone());
    let ccp_cookie_csrf =
        ccp_cookie_csrf(*b"01234567012345670123456701234567", form_finder.clone());

    // Configure router with different CSRF protection endpoints
    let router = Router::new()
        .get(home)
        // Bcrypt-based CSRF protection
        .push(
            Router::with_hoop(bcrypt_csrf)
                .path("bcrypt")
                .get(get_page)
                .post(post_page),
        )
        // HMAC-based CSRF protection
        .push(
            Router::with_hoop(hmac_csrf)
                .path("hmac")
                .get(get_page)
                .post(post_page),
        )
        // AES-GCM-based CSRF protection
        .push(
            Router::with_hoop(aes_gcm_cookie_csrf)
                .path("aes_gcm")
                .get(get_page)
                .post(post_page),
        )
        // ChaCha20Poly1305-based CSRF protection
        .push(
            Router::with_hoop(ccp_cookie_csrf)
                .path("ccp")
                .get(get_page)
                .post(post_page),
        );

    // Start server on port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

// Helper function to generate HTML page with CSRF token and message
fn get_page_html(csrf_token: &str, msg: &str) -> String {
    format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Csrf Example</title></head>
    <body>
    <h2>Csrf Exampe: CookieStore</h2>
    <ul>
        <li><a href="/bcrypt/">Bcrypt</a></li>
        <li><a href="/hmac/">Hmac</a></li>
        <li><a href="/aes_gcm/">Aes Gcm</a></li>
        <li><a href="/ccp/">chacha20poly1305</a></li>
    </ul>
    <form action="./" method="post">
        <input type="hidden" name="csrf_token" value="{csrf_token}" />
        <div>
            <label>Message:<input type="text" name="message" /></label>
        </div>
        <button type="submit">Send</button>
    </form>
    <pre>{msg}</pre>
    </body>
    </html>
    "#
    )
}
