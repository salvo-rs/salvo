use salvo::prelude::*;
use salvo_csrf::*;
use serde::{Deserialize, Serialize};

#[handler]
pub async fn home(res: &mut Response) {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Csrf SessionStore</title></head>
    <body>
    <h2>Csrf Exampe: SessionStore</h2>
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
    let html = get_page_html(depot.csrf_token().map(|s| &**s).unwrap_or_default());
    res.render(Text::Html(html));
}

#[handler]
pub async fn post_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    #[derive(Deserialize, Serialize, Debug)]
    struct Data {
        csrf: String,
        count: usize,
    }
    let mut data = req.parse_json::<Data>().await.unwrap();
    tracing::info!("posted data: {:?}", data);
    data.count += 1;
    data.csrf = depot.csrf_token().cloned().unwrap();
    res.render(Json(data));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let json_finder = JsonFinder::new().with_field_name("csrf");

    let bcrypt_csrf = bcrypt_session_csrf(json_finder.clone());
    let hmac_csrf = hmac_session_csrf(*b"01234567012345670123456701234567", json_finder.clone());
    let aes_gcm_session_csrf = aes_gcm_session_csrf(*b"01234567012345670123456701234567", json_finder.clone());
    let ccp_session_csrf = ccp_session_csrf(*b"01234567012345670123456701234567", json_finder.clone());

    let session_handler = salvo_session::SessionHandler::builder(
        salvo_session::MemoryStore::new(),
        b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
    )
    .build()
    .unwrap();
    let router = Router::new()
        .get(home).hoop(session_handler)
        .push(
            Router::with_hoop(bcrypt_csrf)
                .path("bcrypt")
                .get(get_page)
                .post(post_page),
        )
        .push(Router::with_hoop(hmac_csrf).path("hmac").get(get_page).post(post_page))
        .push(
            Router::with_hoop(aes_gcm_session_csrf)
                .path("aes_gcm")
                .get(get_page)
                .post(post_page),
        )
        .push(
            Router::with_hoop(ccp_session_csrf)
                .path("ccp")
                .get(get_page)
                .post(post_page),
        );
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

fn get_page_html(csrf_token: &str) -> String {
    format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Csrf SessionStore</title></head>
    <body>
    <h2>Csrf Exampe: SessionStore</h2>
    <ul>
        <li><a href="../bcrypt/">Bcrypt</a></li>
        <li><a href="../hmac/">Hmac</a></li>
        <li><a href="../aes_gcm/">Aes Gcm</a></li>
        <li><a href="../ccp/">chacha20poly1305</a></li>
    </ul>
    <script>
    // Get the CSRF value from our cookie.
    let csrfValue = "{}";
    function submit() {{
        let request = new Request("./", {{
            method: "POST",
            // Actix strictly requires the content type to be set.
            headers: {{
                "Content-Type": "application/json",
            }},
            // Set the CSRF token in the request body.
            body: JSON.stringify({{
                csrf: csrfValue,
                count: 0,
            }})
        }});
        fetch(request)
            .then(resp => resp.json()).then(resp => {{
                console.log(resp);
                csrfValue = resp.csrf;
            }});
    }}
    </script>
    <button onclick="submit()">Click me!</button>
    </body>
    </html>
    "#,
        csrf_token
    )
}
