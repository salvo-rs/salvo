//port from https://github.com/seanmonstar/warp/blob/master/examples/todos.rs


use once_cell::sync::Lazy;
use tracing;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

use salvo::prelude::*;

use self::models::*;

static DB: Lazy<Db> = Lazy::new(|| blank_db());

/// Provides a RESTful web server managing some Todos.
///
/// API will be:
///
/// - `GET /todos`: return a JSON list of Todos.
/// - `POST /todos`: create a new Todo.
/// - `PUT /todos/:id`: update a specific Todo.
/// - `DELETE /todos/:id`: delete a specific Todo.
#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "todos=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();

    // View access logs by setting `RUST_LOG=todos`.
    let router = Router::new().path("todos").get(list_todos).post(create_todo).push(
        Router::new().path("<id>").put(update_todo).delete(delete_todo)
    );
    // Start up the server...
    Server::new(router).bind(([0, 0, 0, 0], 3040)).await;
}

#[fn_handler]
pub async fn list_todos(req: &mut Request, res: &mut Response) {
    let opts = req.read::<ListOptions>().await.unwrap();
    // Just return a JSON array of todos, applying the limit and offset.
    let todos = DB.lock().await;
    let todos: Vec<Todo> = todos
        .clone()
        .into_iter()
        .skip(opts.offset.unwrap_or(0))
        .take(opts.limit.unwrap_or(std::usize::MAX))
        .collect();
    res.render_json(&todos);
}

#[fn_handler]
pub async fn create_todo(req: &mut Request, res: &mut Response) {
    let create = req.read::<Todo>().await.unwrap();
    tracing::debug!("create_todo: {:?}", create);

    let mut vec = DB.lock().await;

    for todo in vec.iter() {
        if todo.id == create.id {
            tracing::debug!("    -> id already exists: {}", create.id);
            // Todo with id already exists, return `400 BadRequest`.
            res.set_status_code(StatusCode::BAD_REQUEST);
            return;
        }
    }

    // No existing Todo with id, so insert and return `201 Created`.
    vec.push(create);
    res.set_status_code(StatusCode::CREATED);
}

#[fn_handler]
pub async fn update_todo(req: &mut Request, res: &mut Response) {
    let id = req.get_param::<u64>("id").unwrap();
    let update = req.read::<Todo>().await.unwrap();
    tracing::debug!("update_todo: id={}, todo={:?}", id, update);
    let mut vec = DB.lock().await;

    // Look for the specified Todo...
    for todo in vec.iter_mut() {
        if todo.id == id {
            *todo = update;
            res.set_status_code(StatusCode::OK);
            return;
        }
    }

    tracing::debug!("    -> todo id not found!");

    // If the for loop didn't return OK, then the ID doesn't exist...
    res.set_status_code(StatusCode::NOT_FOUND);
}

#[fn_handler]
pub async fn delete_todo(req: &mut Request, res: &mut Response) {
    let id = req.get_param::<u64>("id").unwrap();
    tracing::debug!("delete_todo: id={}", id);

    let mut vec = DB.lock().await;

    let len = vec.len();
    vec.retain(|todo| {
        // Retain all Todos that aren't this id...
        // In other words, remove all that *are* this id...
        todo.id != id
    });

    // If the vec is smaller, we found and deleted a Todo!
    let deleted = vec.len() != len;

    if deleted {
        // respond with a `204 No Content`, which means successful,
        // yet no body expected...
        res.set_status_code(StatusCode::NO_CONTENT);
    } else {
        tracing::debug!("    -> todo id not found!");
        res.set_status_code(StatusCode::NOT_FOUND);
    }
}

mod models {
    use serde_derive::{Deserialize, Serialize};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// So we don't have to tackle how different database work, we'll just use
    /// a simple in-memory DB, a vector synchronized by a mutex.
    pub type Db = Arc<Mutex<Vec<Todo>>>;

    pub fn blank_db() -> Db {
        Arc::new(Mutex::new(Vec::new()))
    }

    #[derive(Debug, Deserialize, Serialize, Clone)]
    pub struct Todo {
        pub id: u64,
        pub text: String,
        pub completed: bool,
    }

    // The query parameters for list_todos.
    #[derive(Debug, Deserialize)]
    pub struct ListOptions {
        pub offset: Option<usize>,
        pub limit: Option<usize>,
    }
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use reqwest::Client;

    use super::{
        filters,
        models::{self, Todo},
    };

    #[tokio::test]
    async fn test_post() {
        let client = Client::new();
        let resp = client.post("https://127.0.0.1:3030/todos")
            .json(&Todo {
                id: 1,
                text: "test 1".into(),
                completed: false,
            })
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_post_conflict() {
        let client = Client::new();
        let resp = client.post("https://127.0.0.1:3030/todos")
            .json(&Todo {
                id: 1,
                text: "test 1".into(),
                completed: false,
            })
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    fn todo1() -> Todo {
        Todo {
            id: 1,
            text: "test 1".into(),
            completed: false,
        }
    }
}
