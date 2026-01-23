use salvo::macros::Extractible;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

/// Middleware that injects user information into depot.
/// In real applications, this could be authentication middleware
/// that validates tokens and injects user data.
#[handler]
async fn inject_user(depot: &mut Depot) {
    // Simulate injecting authenticated user data into depot
    depot.insert("user_id", 42i64);
    depot.insert("user_role", "admin".to_string());
    depot.insert("is_verified", true);
}

#[handler]
async fn show(req: &mut Request, res: &mut Response) {
    let content = format!(
        r#"<!DOCTYPE html>
    <html>
        <head>
            <title>Parse data</title>
        </head>
        <body>
            <h1>Hello, fill your profile</h1>
            <form action="/{}?username=jobs" method="post">
                <label>First Name:</label><input type="text" name="first_name" />
                <label>Last Name:</label><input type="text" name="last_name" />
                <legend>What is Your Favorite Pet?</legend>
                <input type="checkbox" name="lovers" value="Cats">Cats<br>
                <input type="checkbox" name="lovers" value="Dogs">Dogs<br>
                <input type="checkbox" name="lovers" value="Birds">Birds<br>
                <input type="submit" value="Submit" />
            </form>
        </body>
    </html>
    "#,
        req.params().get("id").unwrap()
    );
    res.render(Text::Html(content));
}
#[handler]
async fn edit(req: &mut Request, depot: &mut Depot) -> String {
    let bad_man: BadMan = req.extract(depot).await.unwrap();
    let bad_man = format!("Bad Man: {bad_man:#?}");
    let good_man: GoodMan = req.extract(depot).await.unwrap();
    let good_man = format!("Good Man: {good_man:#?}");
    // Extract user context from depot (injected by middleware)
    let user_context: UserContext = req.extract(depot).await.unwrap();
    let user_context = format!("User Context (from depot): {user_context:#?}");
    format!("{bad_man}\r\n\r\n\r\n{good_man}\r\n\r\n\r\n{user_context}")
}

#[derive(Serialize, Deserialize, Extractible, Debug)]
#[salvo(extract(
    default_source(from = "query"),
    default_source(from = "param"),
    default_source(from = "body")
))]
struct BadMan<'a> {
    #[serde(default)]
    id: i64,
    username: &'a str,
    first_name: String,
    last_name: &'a str,
    lovers: Vec<String>,
}
#[derive(Serialize, Deserialize, Extractible, Debug)]
#[salvo(extract(
    default_source(from = "query"),
    default_source(from = "param"),
    default_source(from = "body"),
))]
struct GoodMan<'a> {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    username: &'a str,
    first_name: String,
    last_name: &'a str,
    #[salvo(extract(alias = "lovers"))]
    lover: &'a str,
}

/// Struct that extracts user context from depot.
/// This demonstrates extracting data that was injected by middleware.
/// Supports extracting String, &'static str, integers (i8-i128, u8-u128),
/// floats (f32, f64), and bool from depot.
#[derive(Serialize, Deserialize, Extractible, Debug)]
#[salvo(extract(default_source(from = "depot")))]
struct UserContext {
    user_id: i64,
    user_role: String,
    is_verified: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // The inject_user middleware runs before handlers and injects user data into depot
    let router = Router::new()
        .hoop(inject_user)
        .push(Router::with_path("{id}").get(show).post(edit));

    println!("Example url: http://0.0.0.0:8698/95");
    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
