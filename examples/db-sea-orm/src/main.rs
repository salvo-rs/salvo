use std::env;

use entity::post;
use migration::{Migrator, MigratorTrait};
use salvo::affix;
use salvo::prelude::*;
use salvo::serve_static::StaticDir;
use salvo::writer::Text;
use sea_orm::{entity::*, query::*, DatabaseConnection};
use tera::Tera;

mod entity;
mod migration;

const DEFAULT_POSTS_PER_PAGE: u64 = 5;
type Result<T> = std::result::Result<T, StatusError>;

#[derive(Clone, Debug)]
struct AppState {
    templates: tera::Tera,
    conn: DatabaseConnection,
}

#[handler]
async fn create(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let state = depot
        .obtain::<AppState>()
        .ok_or_else(StatusError::internal_server_error)?;
    let form = req
        .parse_form::<post::Model>()
        .await
        .map_err(|_| StatusError::bad_request())?;
    post::ActiveModel {
        title: Set(form.title.to_owned()),
        text: Set(form.text.to_owned()),
        ..Default::default()
    }
    .save(&state.conn)
    .await
    .map_err(|_| StatusError::internal_server_error())?;

    res.render(Redirect::found("/"));
    Ok(())
}

#[handler]
async fn list(req: &mut Request, depot: &mut Depot) -> Result<Text<String>> {
    let state = depot
        .obtain::<AppState>()
        .ok_or_else(StatusError::internal_server_error)?;
    let page = req.query("page").unwrap_or(1);
    let posts_per_page = req.query("posts_per_page").unwrap_or(DEFAULT_POSTS_PER_PAGE) as usize;
    let paginator = post::Entity::find()
        .order_by_asc(post::Column::Id)
        .paginate(&state.conn, posts_per_page);
    let num_pages = paginator.num_pages().await.map_err(|_| StatusError::bad_request())?;
    let posts = paginator
        .fetch_page(page - 1)
        .await
        .map_err(|_| StatusError::internal_server_error())?;

    let mut ctx = tera::Context::new();
    ctx.insert("posts", &posts);
    ctx.insert("page", &page);
    ctx.insert("posts_per_page", &posts_per_page);
    ctx.insert("num_pages", &num_pages);

    let body = state
        .templates
        .render("index.html.tera", &ctx)
        .map_err(|_| StatusError::internal_server_error())?;
    Ok(Text::Html(body))
}

#[handler]
async fn new(depot: &mut Depot) -> Result<Text<String>> {
    let state = depot
        .obtain::<AppState>()
        .ok_or_else(StatusError::internal_server_error)?;
    let ctx = tera::Context::new();
    let body = state
        .templates
        .render("new.html.tera", &ctx)
        .map_err(|_| StatusError::internal_server_error())?;
    Ok(Text::Html(body))
}

#[handler]
async fn edit(req: &mut Request, depot: &mut Depot) -> Result<Text<String>> {
    let state = depot
        .obtain::<AppState>()
        .ok_or_else(StatusError::internal_server_error)?;
    let id = req.param::<i32>("id").unwrap_or_default();
    let post: post::Model = post::Entity::find_by_id(id)
        .one(&state.conn)
        .await
        .map_err(|_| StatusError::internal_server_error())?
        .ok_or_else(StatusError::not_found)?;

    let mut ctx = tera::Context::new();
    ctx.insert("post", &post);

    let body = state
        .templates
        .render("edit.html.tera", &ctx)
        .map_err(|_| StatusError::internal_server_error())?;
    Ok(Text::Html(body))
}

#[handler]
async fn update(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let state = depot
        .obtain::<AppState>()
        .ok_or_else(StatusError::internal_server_error)?;
    let id = req.param::<i32>("id").unwrap_or_default();
    let form = req
        .parse_form::<post::Model>()
        .await
        .map_err(|_| StatusError::bad_request())?;
    post::ActiveModel {
        id: Set(id),
        title: Set(form.title.to_owned()),
        text: Set(form.text.to_owned()),
    }
    .save(&state.conn)
    .await
    .map_err(|_| StatusError::internal_server_error())?;
    res.render(Redirect::found("/"));
    Ok(())
}

#[handler]
async fn delete(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let state = depot
        .obtain::<AppState>()
        .ok_or_else(StatusError::internal_server_error)?;
    let id = req.param::<i32>("id").unwrap_or_default();
    let post: post::ActiveModel = post::Entity::find_by_id(id)
        .one(&state.conn)
        .await
        .map_err(|_| StatusError::internal_server_error())?
        .ok_or_else(StatusError::not_found)?
        .into();
    post.delete(&state.conn)
        .await
        .map_err(|_| StatusError::internal_server_error())?;

    res.render(Redirect::found("/"));
    Ok(())
}

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "debug");
    tracing_subscriber::fmt::init();

    let db_url = "sqlite::memory:";
    let server_url = "0.0.0.0:7878";

    // create post table if not exists
    let conn = sea_orm::Database::connect(db_url).await.unwrap();
    Migrator::up(&conn, None).await.unwrap();
    let templates = Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
    let state = AppState { templates, conn };

    println!("Starting server at {}", server_url);

    let router = Router::new()
        .hoop(affix::inject(state))
        .post(create)
        .get(list)
        .push(Router::with_path("new").get(new))
        .push(Router::with_path("<id>").get(edit).post(update))
        .push(Router::with_path("delete/<id>").post(delete))
        .push(Router::with_path("static/<**>").get(StaticDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static"))));

    let acceptor = TcpListener::new(&server_url).bind().await;
    Server::new(acceptor).serve(router).await;
}
