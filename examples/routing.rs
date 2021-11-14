use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let debug_mode = true;
    let admin_mode = true;
    let router = Router::new()
        .get(index)
        .push(
            Router::with_path("users")
                .hoop(auth)
                .post(create_user)
                .push(Router::with_path(r"<id:num>").post(update_user).delete(delete_user)),
        )
        .push(
            Router::with_path("users")
                .get(list_users)
                .push(Router::with_path(r"<id:num>").get(show_user)),
        )
        .then(|router| {
            if debug_mode {
                router.push(Router::with_path("debug").get(debug))
            } else {
                router
            }
        })
        .then(|router| {
            if admin_mode {
                router.push(Router::with_path("admin").get(admin))
            } else {
                router
            }
        });

    Server::new(TcpListener::bind(([0, 0, 0, 0], 7878))).serve(router).await;
}

#[fn_handler]
async fn admin(res: &mut Response) {
    res.render_plain_text("Admin page");
}
#[fn_handler]
async fn debug(res: &mut Response) {
    res.render_plain_text("Debug page");
}
#[fn_handler]
async fn index(res: &mut Response) {
    res.render_plain_text("Hello world!");
}
#[fn_handler]
async fn auth(res: &mut Response) {
    res.render_plain_text("user has authed\n\n");
}
#[fn_handler]
async fn list_users(res: &mut Response) {
    res.render_plain_text("list users");
}
#[fn_handler]
async fn show_user(res: &mut Response) {
    res.render_plain_text("show user");
}
#[fn_handler]
async fn create_user(res: &mut Response) {
    res.render_plain_text("user created");
}
#[fn_handler]
async fn update_user(res: &mut Response) {
    res.render_plain_text("user updated");
}
#[fn_handler]
async fn delete_user(res: &mut Response) {
    res.render_plain_text("user deleted");
}
