use std::env;

use entity::post;
use migration::{Migrator, MigratorTrait};
use salvo::prelude::*;
use salvo::serve_static::StaticDir;
use salvo::writing::Text;
use sea_orm::{DatabaseConnection, entity::*, query::*};
use tera::Tera;

mod entity;
mod migration;

// Default number of posts to display per page
const DEFAULT_POSTS_PER_PAGE: u64 = 5;
type Result<T> = std::result::Result<T, StatusError>;

// Application state containing database connection and template engine
#[derive(Clone, Debug)]
struct AppState {
    templates: tera::Tera,
    conn: DatabaseConnection,
}

// Handler for creating a new blog post
#[handler]
async fn create(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let state = depot
        .obtain::<AppState>()
        .map_err(|_| StatusError::internal_server_error())?;
    // Parse form data into post model
    let form = req
        .parse_form::<post::Model>()
        .await
        .map_err(|_| StatusError::bad_request())?;
    // Create new post in database
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

// Handler for listing blog posts with pagination
#[handler]
async fn list(req: &mut Request, depot: &mut Depot) -> Result<Text<String>> {
    let state = depot
        .obtain::<AppState>()
        .map_err(|_| StatusError::internal_server_error())?;
    // Get pagination parameters from query
    let page = req.query("page").unwrap_or(1);
    let posts_per_page = req
        .query("posts_per_page")
        .unwrap_or(DEFAULT_POSTS_PER_PAGE);

    // Create paginator for posts
    let paginator = post::Entity::find()
        .order_by_asc(post::Column::Id)
        .paginate(&state.conn, posts_per_page);

    // Get total number of pages
    let num_pages = paginator
        .num_pages()
        .await
        .map_err(|_| StatusError::bad_request())?;

    // Get posts for current page
    let posts = paginator
        .fetch_page(page - 1)
        .await
        .map_err(|_| StatusError::internal_server_error())?;

    // Render template with posts and pagination data
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

// Handler for displaying new post form
#[handler]
async fn new(depot: &mut Depot) -> Result<Text<String>> {
    let state = depot
        .obtain::<AppState>()
        .map_err(|_| StatusError::internal_server_error())?;
    let ctx = tera::Context::new();
    let body = state
        .templates
        .render("new.html.tera", &ctx)
        .map_err(|_| StatusError::internal_server_error())?;
    Ok(Text::Html(body))
}

// Handler for displaying edit post form
#[handler]
async fn edit(req: &mut Request, depot: &mut Depot) -> Result<Text<String>> {
    let state = depot
        .obtain::<AppState>()
        .map_err(|_| StatusError::internal_server_error())?;
    // Get post ID from path parameters
    let id = req.param::<i32>("id").unwrap_or_default();
    // Find post in database
    let post: post::Model = post::Entity::find_by_id(id)
        .one(&state.conn)
        .await
        .map_err(|_| StatusError::internal_server_error())?
        .ok_or_else(StatusError::not_found)?;

    // Render edit form with post data
    let mut ctx = tera::Context::new();
    ctx.insert("post", &post);

    let body = state
        .templates
        .render("edit.html.tera", &ctx)
        .map_err(|_| StatusError::internal_server_error())?;
    Ok(Text::Html(body))
}

// Handler for updating an existing post
#[handler]
async fn update(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let state = depot
        .obtain::<AppState>()
        .map_err(|_| StatusError::internal_server_error())?;
    // Get post ID and form data
    let id = req.param::<i32>("id").unwrap_or_default();
    let form = req
        .parse_form::<post::Model>()
        .await
        .map_err(|_| StatusError::bad_request())?;

    // Update post in database
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

// Handler for deleting a post
#[handler]
async fn delete(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let state = depot
        .obtain::<AppState>()
        .map_err(|_| StatusError::internal_server_error())?;
    // Get post ID and find post
    let id = req.param::<i32>("id").unwrap_or_default();
    let post: post::ActiveModel = post::Entity::find_by_id(id)
        .one(&state.conn)
        .await
        .map_err(|_| StatusError::internal_server_error())?
        .ok_or_else(StatusError::not_found)?
        .into();

    // Delete post from database
    post.delete(&state.conn)
        .await
        .map_err(|_| StatusError::internal_server_error())?;

    res.render(Redirect::found("/"));
    Ok(())
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Database and server configuration
    let db_url = "sqlite::memory:";
    let server_url = "0.0.0.0:5800";

    // create post table if not exists
    let conn = sea_orm::Database::connect(db_url).await.unwrap();
    Migrator::up(&conn, None).await.unwrap();

    // Initialize template engine
    let templates = Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
    let state = AppState { templates, conn };

    println!("Starting server at {server_url}");

    // Configure router with all handlers
    let router = Router::new()
        .hoop(affix_state::inject(state))
        .post(create)
        .get(list)
        .push(Router::with_path("new").get(new))
        .push(Router::with_path("{id}").get(edit).post(update))
        .push(Router::with_path("delete/{id}").post(delete))
        .push(Router::with_path("static/{**}").get(StaticDir::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/static"
        ))));

    // Start server
    let acceptor = TcpListener::new(&server_url).bind().await;
    Server::new(acceptor).serve(router).await;
}
