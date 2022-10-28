# CSRF

Middleware that provides CSRF (Cross-Site Request Forgery) protection.

## The main function

* `CsrfStore` provides access to data. `CookieStore` will store data in `Cookie`, and will verify the validity of the request according to the `csrf-token` and `Cookie` values submitted by the user. The data is stored in the `Session`, and the validity of the request is verified with the data submitted by the user and the data in the `Session`. Note that the `SessionStore` must be used with the `session` function.

* `Csrf` is a structure that implements `Handler`, and there is a `skipper` field inside, which can be specified to skip certain requests that do not require authentication. By default, `Method::POST`, `Method: :PATCH`, `Method::DELETE`, `Method::PUT` requests.
## Config Cargo.toml

```toml
salvo = { version = "*", features = ["csrf"] }
```

## Sample Code

```rust
use salvo::prelude::*;
use salvo_csrf::*;
use serde::{Deserialize, Serialize};

#[handler]
pub async fn home(res: &mut Response) {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Csrf CookieStore</title></head>
    <body>
    <h2>Csrf Exampe: CookieStore</h2>
    <ul>
        <li><a href="../bcrypt">Bcrypt</a></li>
        <li><a href="../hmac">Hmac</a></li>
        <li><a href="../aes_gcm">Aes Gcm</a></li>
        <li><a href="../ccp">chacha20poly1305</a></li>
    </ul>
    </body>"#;
    res.render(Text::Html(html));
}

#[handler]
pub async fn get_page(depot: &mut Depot, res: &mut Response) {
    let new_token = depot.csrf_token().map(|s| &**s).unwrap_or_default();
    res.render(Text::Html(get_page_html(new_token, "")));
}

#[handler]
pub async fn post_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    #[derive(Deserialize, Serialize, Debug)]
    struct Data {
        csrf_token: String,
        message: String,
    }
    let data = req.parse_form::<Data>().await.unwrap();
    tracing::info!("posted data: {:?}", data);
    let new_token = depot.csrf_token().map(|s| &**s).unwrap_or_default();
    let html = get_page_html(new_token, &format!("{:#?}", data));
    res.render(Text::Html(html));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let form_finder = FormFinder::new("csrf_token");

    let bcrypt_csrf = bcrypt_cookie_csrf(form_finder.clone());
    let hmac_csrf = hmac_cookie_csrf(*b"01234567012345670123456701234567", form_finder.clone());
    let aes_gcm_cookie_csrf = aes_gcm_cookie_csrf(*b"01234567012345670123456701234567", form_finder.clone());
    let ccp_cookie_csrf = ccp_cookie_csrf(*b"01234567012345670123456701234567", form_finder.clone());

    let router = Router::new()
        .get(home)
        .push(
            Router::with_hoop(bcrypt_csrf)
                .path("bcrypt")
                .get(get_page)
                .post(post_page),
        )
        .push(Router::with_hoop(hmac_csrf).path("hmac").get(get_page).post(post_page))
        .push(
            Router::with_hoop(aes_gcm_cookie_csrf)
                .path("aes_gcm")
                .get(get_page)
                .post(post_page),
        )
        .push(
            Router::with_hoop(ccp_cookie_csrf)
                .path("ccp")
                .get(get_page)
                .post(post_page),
        );
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}

fn get_page_html(csrf_token: &str, msg: &str) -> String {
    format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Csrf Example</title></head>
    <body>
    <h2>Csrf Exampe: CookieStore</h2>
    <ul>
        <li><a href="../bcrypt/">Bcrypt</a></li>
        <li><a href="../hmac/">Hmac</a></li>
        <li><a href="../aes_gcm/">Aes Gcm</a></li>
        <li><a href="../ccp/">chacha20poly1305</a></li>
    </ul>
    <form action="./" method="post">
        <input type="hidden" name="csrf_token" value="{}" />
        <div>
            <label>Message:<input type="text" name="message" /></label>
        </div>
        <button type="submit">Send</button>
    </form>
    <pre>{}</pre>
    </body>
    </html>
    "#,
        csrf_token, msg
    )
}
```