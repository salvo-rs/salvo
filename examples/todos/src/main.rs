use std::sync::LazyLock;

use salvo::prelude::*;
use salvo::size_limiter;

use self::models::*;

static STORE: LazyLock<Db> = LazyLock::new(new_store);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    start_server().await;
}

pub(crate) async fn start_server() {
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(route()).await;
}

#[handler]
fn index(res: &mut Response) {
    res.render(Text::Html(
        "<html>
            <body>
                <a href=\"/todos\">going to the todo page</a>
            </body>
        </html>",
    ));
}

fn route() -> Router {
    Router::new()
        .push(Router::new().get(index))
        .push(Router::new().path("todos").push(todo_route()))
}

fn todo_route() -> Router {
    Router::new()
        .hoop(size_limiter::max_size(1024 * 16))
        .get(list_todos)
        .post(create_todo)
        .push(
            Router::with_path("{id}")
                .put(update_todo)
                .delete(delete_todo),
        )
}

#[handler]
pub async fn list_todos(req: &mut Request, res: &mut Response) {
    let opts = req.parse_body::<ListOptions>().await.unwrap_or_default();
    let todos = STORE.lock().await;
    let todos: Vec<Todo> = todos
        .clone()
        .into_iter()
        .skip(opts.offset.unwrap_or(0))
        .take(opts.limit.unwrap_or(usize::MAX))
        .collect();
    res.render(Json(todos));
}

#[handler]
pub async fn create_todo(req: &mut Request, res: &mut Response) {
    let new_todo = req.parse_body::<Todo>().await.unwrap();
    tracing::debug!(todo = ?new_todo, "create todo");

    let mut vec = STORE.lock().await;

    for todo in vec.iter() {
        if todo.id == new_todo.id {
            tracing::debug!(id = ?new_todo.id, "todo already exists");
            res.status_code(StatusCode::BAD_REQUEST);
            return;
        }
    }

    vec.push(new_todo);
    res.status_code(StatusCode::CREATED);
}

#[handler]
pub async fn update_todo(req: &mut Request, res: &mut Response) {
    let id = req.param::<u64>("id").unwrap();
    let updated_todo = req.parse_body::<Todo>().await.unwrap();
    tracing::debug!(todo = ?updated_todo, id = ?id, "update todo");
    let mut vec = STORE.lock().await;

    for todo in vec.iter_mut() {
        if todo.id == id {
            *todo = updated_todo;
            res.status_code(StatusCode::OK);
            return;
        }
    }

    tracing::debug!(?id, "todo is not found");
    res.status_code(StatusCode::NOT_FOUND);
}

#[handler]
pub async fn delete_todo(req: &mut Request, res: &mut Response) {
    let id = req.param::<u64>("id").unwrap();
    tracing::debug!(?id, "delete todo");

    let mut vec = STORE.lock().await;

    let len = vec.len();
    vec.retain(|todo| todo.id != id);

    let deleted = vec.len() != len;
    if deleted {
        res.status_code(StatusCode::NO_CONTENT);
    } else {
        tracing::debug!(?id, "todo is not found");
        res.status_code(StatusCode::NOT_FOUND);
    }
}

mod models {
    use serde::{Deserialize, Serialize};
    use tokio::sync::Mutex;

    pub type Db = Mutex<Vec<Todo>>;

    pub fn new_store() -> Db {
        Mutex::new(Vec::new())
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Todo {
        pub id: u64,
        pub text: String,
        pub completed: bool,
    }

    #[derive(Deserialize, Debug, Default)]
    pub struct ListOptions {
        pub offset: Option<usize>,
        pub limit: Option<usize>,
    }
}

#[cfg(test)]
mod tests {
    use salvo::http::StatusCode;
    use salvo::test::TestClient;

    use super::models::Todo;

    #[tokio::test]
    async fn test_todo_create() {
        tokio::task::spawn(async {
            super::start_server().await;
        });
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let res = TestClient::post("http://0.0.0.0:5800/todos")
            .json(&test_todo())
            .send(super::route())
            .await;

        assert_eq!(res.status_code.unwrap(), StatusCode::CREATED);
        let res = TestClient::post("http://0.0.0.0:5800/todos")
            .json(&test_todo())
            .send(super::route())
            .await;

        assert_eq!(res.status_code.unwrap(), StatusCode::BAD_REQUEST);
    }

    fn test_todo() -> Todo {
        Todo {
            id: 1,
            text: "test todo".into(),
            completed: false,
        }
    }
}
