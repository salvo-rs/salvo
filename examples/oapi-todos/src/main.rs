use once_cell::sync::Lazy;
use salvo::oapi::extract::*;
use salvo::prelude::*;

use self::models::*;

static STORE: Lazy<Db> = Lazy::new(new_store);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().push(
        Router::with_path("api").push(
            Router::with_path("todos")
                .get(list_todos)
                .post(create_todo)
                .push(Router::with_path("<id>").patch(update_todo).delete(delete_todo)),
        ),
    );

    let doc = OpenApi::new("todos api", "0.0.1").merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("/"));

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

/// List todos.
#[endpoint(
    parameters(
        ("offset", description = "Offset is an optional query paramter."),
    )
)]
pub async fn list_todos(offset: QueryParam<usize, false>, limit: QueryParam<usize, false>) -> Json<Vec<Todo>> {
    let todos = STORE.lock().await;
    let todos: Vec<Todo> = todos
        .clone()
        .into_iter()
        .skip(offset.into_inner().unwrap_or(0))
        .take(limit.into_inner().unwrap_or(std::usize::MAX))
        .collect();
    Json(todos)
}

/// Create new todo.
#[endpoint(status_codes(201, 409))]
pub async fn create_todo(new_todo: JsonBody<Todo>) -> Result<StatusCode, StatusError> {
    tracing::debug!(todo = ?new_todo, "create todo");

    let mut vec = STORE.lock().await;

    for todo in vec.iter() {
        if todo.id == new_todo.id {
            tracing::debug!(id = ?new_todo.id, "todo already exists");
            return Err(StatusError::bad_request().brief("todo already exists"));
        }
    }

    vec.push(new_todo.into_inner());
    Ok(StatusCode::CREATED)
}

/// Update existing todo.
#[endpoint(status_codes(200, 404))]
pub async fn update_todo(id: PathParam<u64>, updated: JsonBody<Todo>) -> Result<StatusCode, StatusError> {
    tracing::debug!(todo = ?updated, id = ?id, "update todo");
    let mut vec = STORE.lock().await;

    for todo in vec.iter_mut() {
        if todo.id == *id {
            *todo = (*updated).clone();
            return Ok(StatusCode::OK);
        }
    }

    tracing::debug!(id = ?id, "todo is not found");
    Err(StatusError::not_found())
}

/// Delete todo.
#[endpoint(status_codes(200, 401, 404))]
pub async fn delete_todo(id: PathParam<u64>) -> Result<StatusCode, StatusError> {
    tracing::debug!(id = ?id, "delete todo");

    let mut vec = STORE.lock().await;

    let len = vec.len();
    vec.retain(|todo| todo.id != *id);

    let deleted = vec.len() != len;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        tracing::debug!(id = ?id, "todo is not found");
        Err(StatusError::not_found())
    }
}

mod models {
    use salvo::oapi::ToSchema;
    use serde::{Deserialize, Serialize};
    use tokio::sync::Mutex;

    pub type Db = Mutex<Vec<Todo>>;

    pub fn new_store() -> Db {
        Mutex::new(Vec::new())
    }

    #[derive(Serialize, Deserialize, ToSchema)]
    pub(super) enum TodoError {
        /// Happens when Todo item already exists
        Config(String),
        /// Todo not found from storage
        NotFound(String),
    }

    #[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
    pub struct Todo {
        #[salvo(schema(example = 1))]
        pub id: u64,
        #[salvo(schema(example = "Buy coffee"))]
        pub text: String,
        pub completed: bool,
    }
}
