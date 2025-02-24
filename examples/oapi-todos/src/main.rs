use std::sync::LazyLock;

use salvo::oapi::{ToSchema, extract::*};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

static STORE: LazyLock<Db> = LazyLock::new(new_store);
pub type Db = Mutex<Vec<Todo>>;

pub fn new_store() -> Db {
    Mutex::new(Vec::new())
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct Todo {
    #[salvo(schema(example = 1))]
    pub id: u64,
    #[salvo(schema(example = "Buy coffee"))]
    pub text: String,
    pub completed: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index).push(
        Router::with_path("api").push(
            Router::with_path("todos")
                .get(list_todos)
                .post(create_todo)
                .push(
                    Router::with_path("{id}")
                        .patch(update_todo)
                        .delete(delete_todo),
                ),
        ),
    );

    let doc = OpenApi::new("todos api", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(
            SwaggerUi::new("/api-doc/openapi.json")
                .title("Todos - SwaggerUI")
                .into_router("/swagger-ui"),
        )
        .unshift(
            Scalar::new("/api-doc/openapi.json")
                .title("Todos - Scalar")
                .into_router("/scalar"),
        )
        .unshift(
            RapiDoc::new("/api-doc/openapi.json")
                .title("Todos - RapiDoc")
                .into_router("/rapidoc"),
        )
        .unshift(
            ReDoc::new("/api-doc/openapi.json")
                .title("Todos - ReDoc")
                .into_router("/redoc"),
        );

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[handler]
pub async fn index() -> Text<&'static str> {
    Text::Html(INDEX_HTML)
}

/// List todos.
#[endpoint(
    tags("todos"),
    parameters(
        ("offset", description = "Offset is an optional query paramter."),
    )
)]
pub async fn list_todos(
    offset: QueryParam<usize, false>,
    limit: QueryParam<usize, false>,
) -> Json<Vec<Todo>> {
    let todos = STORE.lock().await;
    let todos: Vec<Todo> = todos
        .clone()
        .into_iter()
        .skip(offset.into_inner().unwrap_or(0))
        .take(limit.into_inner().unwrap_or(usize::MAX))
        .collect();
    Json(todos)
}

/// Create new todo.
#[endpoint(tags("todos"), status_codes(201, 409))]
pub async fn create_todo(req: JsonBody<Todo>) -> Result<StatusCode, StatusError> {
    tracing::debug!(todo = ?req, "create todo");

    let mut vec = STORE.lock().await;

    for todo in vec.iter() {
        if todo.id == req.id {
            tracing::debug!(id = ?req.id, "todo already exists");
            return Err(StatusError::bad_request().brief("todo already exists"));
        }
    }

    vec.push(req.into_inner());
    Ok(StatusCode::CREATED)
}

/// Update existing todo.
#[endpoint(tags("todos"), status_codes(200, 404))]
pub async fn update_todo(
    id: PathParam<u64>,
    updated: JsonBody<Todo>,
) -> Result<StatusCode, StatusError> {
    tracing::debug!(todo = ?updated, id = ?id, "update todo");
    let mut vec = STORE.lock().await;

    for todo in vec.iter_mut() {
        if todo.id == *id {
            *todo = (*updated).clone();
            return Ok(StatusCode::OK);
        }
    }

    tracing::debug!(?id, "todo is not found");
    Err(StatusError::not_found())
}

/// Delete todo.
#[endpoint(tags("todos"), status_codes(200, 401, 404))]
pub async fn delete_todo(id: PathParam<u64>) -> Result<StatusCode, StatusError> {
    tracing::debug!(?id, "delete todo");

    let mut vec = STORE.lock().await;

    let len = vec.len();
    vec.retain(|todo| todo.id != *id);

    let deleted = vec.len() != len;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        tracing::debug!(?id, "todo is not found");
        Err(StatusError::not_found())
    }
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Oapi todos</title>
    </head>
    <body>
        <ul>
        <li><a href="swagger-ui" target="_blank">swagger-ui</a></li>
        <li><a href="scalar" target="_blank">scalar</a></li>
        <li><a href="rapidoc" target="_blank">rapidoc</a></li>
        <li><a href="redoc" target="_blank">redoc</a></li>
        </ul>
    </body>
</html>
"#;
