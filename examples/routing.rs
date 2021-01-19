use salvo::prelude::*;

#[tokio::main]
async fn main() {
    let router = Router::new()
        .get(index)
        .push(
            Router::new()
                .path("users")
                .before(auth)
                .post(create_user)
                .push(Router::new().path(r"<id:/\d+/>").post(update_user).delete(delete_user)),
        )
        .push(
            Router::new()
                .path("users")
                .get(list_users)
                .push(Router::new().path(r"<id:/\d+/>").get(show_user)),
        );

    Server::new(router).run(([127, 0, 0, 1], 7878)).await;
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
