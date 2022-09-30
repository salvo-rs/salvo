use salvo::prelude::*;
use salvo_csrf::{Csrf, CsrfDepotExt, HmacCipher, JsonFinder};
use serde::{Deserialize, Serialize};

#[handler]
pub async fn get_login(depot: &mut Depot, res: &mut Response) {
    let html = format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head><meta charset="UTF-8"><title>Example</title></head>
    <body>
    <script>
    // Get the CSRF value from our cookie.
    let csrfValue = "{}";
    function submit() {{
        let request = new Request("/login", {{
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
        depot.csrf_token().map(|s| &**s).unwrap_or_default()
    );
    res.render(Text::Html(html));
}

#[handler]
pub async fn post_login(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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
    let session_handler = salvo_session::SessionHandler::builder(
        salvo_session::MemoryStore::new(),
        b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
    )
    .build()
    .unwrap();
    let csrf = Csrf::new(
        salvo_csrf::session_store(),
        HmacCipher::new(*b"01234567012345670123456701234567"),
    )
    .add_finder(JsonFinder::new("csrf"));
    let router = Router::new()
        .hoop(session_handler)
        .hoop(csrf)
        .push(Router::with_path("login").get(get_login).post(post_login));
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
