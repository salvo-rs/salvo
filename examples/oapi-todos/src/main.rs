use once_cell::sync::Lazy;
use salvo::oapi::extract::*;
use salvo::oapi::swagger_ui::SwaggerUi;
use salvo::oapi::{Info, OpenApi};
use salvo::prelude::*;

use self::models::*;

static STORE: Lazy<Db> = Lazy::new(new_store);

#[handler]
async fn hello(res: &mut Response) {
    res.render("Hello");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello).push(
        Router::with_path("api").push(
            Router::with_path("todos")
                .get(list_todos)
                .post(create_todo)
                .push(Router::with_path("<id>").patch(update_todo).delete(delete_todo)),
        ),
    );

    let doc = OpenApi::new(Info::new("todos api", "0.0.1")).merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[endpoint]
pub async fn list_todos(offset: QueryParam<Option<usize>>, limit: QueryParam<Option<usize>>) -> Json<Vec<Todo>> {
    let todos = STORE.lock().await;
    let todos: Vec<Todo> = todos
        .clone()
        .into_iter()
        .skip(offset.into_inner().unwrap_or(0))
        .take(limit.into_inner().unwrap_or(std::usize::MAX))
        .collect();
    Json(todos)
}

#[endpoint(
    responses(
        (status = 201, description = "Todo created successfully", body = models::Todo),
        (status = 409, description = "Todo already exists", body = TodoError, example = json!(TodoError::Config(String::from("id = 1"))))
    )
)]
pub async fn create_todo(new_todo: JsonBody<Todo>, res: &mut Response) {
    tracing::debug!(todo = ?new_todo, "create todo");

    let mut vec = STORE.lock().await;

    for todo in vec.iter() {
        if todo.id == new_todo.id {
            tracing::debug!(id = ?new_todo.id, "todo already exists");
            res.set_status_code(StatusCode::BAD_REQUEST);
            return;
        }
    }

    vec.push(new_todo.into_inner());
    res.set_status_code(StatusCode::CREATED);
}

#[endpoint(
    responses(
        (status = 200, description = "Todo modified successfully"),
        (status = 404, description = "Todo not found", body = TodoError, example = json!(TodoError::NotFound(String::from("id = 1"))))
    ),
)]
pub async fn update_todo(id: PathParam<u64>, updated_todo: JsonBody<Todo>, res: &mut Response) {
    tracing::debug!(todo = ?updated_todo, id = ?id, "update todo");
    let mut vec = STORE.lock().await;

    for todo in vec.iter_mut() {
        if todo.id == *id {
            *todo = (*updated_todo).clone();
            res.set_status_code(StatusCode::OK);
            return;
        }
    }

    tracing::debug!(id = ?id, "todo is not found");
    res.set_status_code(StatusCode::NOT_FOUND);
}

#[endpoint(
    responses(
        (status = 200, description = "Todo deleted successfully"),
        (status = 401, description = "Unauthorized to delete Todo"),
        (status = 404, description = "Todo not found", body = TodoError, example = json!(TodoError::NotFound(String::from("id = 1"))))
    ),
)]
pub async fn delete_todo(id: PathParam<u64>, res: &mut Response) {
    tracing::debug!(id = ?id, "delete todo");

    let mut vec = STORE.lock().await;

    let len = vec.len();
    vec.retain(|todo| todo.id != *id);

    let deleted = vec.len() != len;
    if deleted {
        res.set_status_code(StatusCode::NO_CONTENT);
    } else {
        tracing::debug!(id = ?id, "todo is not found");
        res.set_status_code(StatusCode::NOT_FOUND);
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
        #[schema(example = 1)]
        pub id: u64,
        #[schema(example = "Buy coffee")]
        pub text: String,
        pub completed: bool,
    }
}
