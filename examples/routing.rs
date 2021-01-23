use salvo::prelude::*;

#[tokio::main]
async fn main() {
    let debug_mode = true;
    let admin_mode = true;
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
        ).push_when(|_|if debug_mode {
            Some(Router::new().path("debug").get(debug))
        } else {
            None
        }).visit(|parent|{
            if admin_mode {
                parent.push(Router::new().path("admin").get(admin))
            } else {
                parent
            }
        });

    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
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
