use once_cell::sync::Lazy;

use salvo::oapi::openapi;
use salvo::prelude::*;
use salvo::size_limiter;

use self::models::*;

// use utoipa::OpenApi;
use salvo::oapi::openapi::OpenApi;
use salvo::oapi::swagger::{Config, SwaggerUi};
use salvo::oapi::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify,
};

static STORE: Lazy<Db> = Lazy::new(new_store);
static API_DOC: Lazy<OpenApi> = Lazy::new(|| {
    OpenApi::new()
        .path(list_todos)
        .path(create_todo)
        .path(delete_todo)
        .path(update_todo)
        .component(schemas(models::Todo, models::TodoError))
        .modifier(&SecurityAddon)
        .tag(Tag::new().name("todo").description("Todo items management endpoints."))
});

#[handler]
async fn hello(res: &mut Response) {
    res.render("Hello");
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap(); // we can unwrap safely since there already is components registered.
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("todo_apikey"))),
        )
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(route()).await;
}

pub(crate) fn route() -> Router {
    let config = Config::from("/api-doc/openapi.json");
    Router::new()
        .get(hello)
        .push(
            Router::with_path("api").push(
                Router::with_path("todos")
                    .hoop(size_limiter::max_size(1024 * 16))
                    .get(list_todos)
                    .post(create_todo)
                    .push(Router::with_path("<id>").put(update_todo).delete(delete_todo)),
            ),
        )
        .push(Router::with_path("/api-doc/openapi.json").get(openapi_json))
        .push(SwaggerUi::new(config).router("swagger-ui"))
}

#[handler]
pub async fn openapi_json(res: &mut Response) {
    res.render(Json(&API_DOC))
}

#[salvo::oapi::endpoint(
    get,
    path = "/api/todos",
    responses(
        (status = 200, description = "List all todos successfully", body = [Todo])
    )
)]
#[handler]
pub async fn list_todos(req: &mut Request, res: &mut Response) {
    let opts = req.parse_body::<ListOptions>().await.unwrap_or_default();
    let todos = STORE.lock().await;
    let todos: Vec<Todo> = todos
        .clone()
        .into_iter()
        .skip(opts.offset.unwrap_or(0))
        .take(opts.limit.unwrap_or(std::usize::MAX))
        .collect();
    res.render(Json(todos));
}

#[salvo::oapi::endpoint(
        post,
        path = "/api/todos",
        request_body = Todo,
        responses(
            (status = 201, description = "Todo created successfully", body = Todo),
            (status = 409, description = "Todo already exists", body = TodoError, example = json!(TodoError::Config(String::from("id = 1"))))
        )
    )]
#[handler]
pub async fn create_todo(req: &mut Request, res: &mut Response) {
    let new_todo = req.parse_body::<Todo>().await.unwrap();
    tracing::debug!(todo = ?new_todo, "create todo");

    let mut vec = STORE.lock().await;

    for todo in vec.iter() {
        if todo.id == new_todo.id {
            tracing::debug!(id = ?new_todo.id, "todo already exists");
            res.set_status_code(StatusCode::BAD_REQUEST);
            return;
        }
    }

    vec.push(new_todo);
    res.set_status_code(StatusCode::CREATED);
}

#[salvo::oapi::endpoint(
        put,
        path = "/api/todos/{id}",
        responses(
            (status = 200, description = "Todo modified successfully"),
            (status = 404, description = "Todo not found", body = TodoError, example = json!(TodoError::NotFound(String::from("id = 1"))))
        ),
        params(
            ("id" = i32, Path, description = "Id of todo item to modify")
        )
    )]
#[handler]
pub async fn update_todo(req: &mut Request, res: &mut Response) {
    let id = req.param::<u64>("id").unwrap();
    let updated_todo = req.parse_body::<Todo>().await.unwrap();
    tracing::debug!(todo = ?updated_todo, id = ?id, "update todo");
    let mut vec = STORE.lock().await;

    for todo in vec.iter_mut() {
        if todo.id == id {
            *todo = updated_todo;
            res.set_status_code(StatusCode::OK);
            return;
        }
    }

    tracing::debug!(id = ?id, "todo is not found");
    res.set_status_code(StatusCode::NOT_FOUND);
}

#[salvo::oapi::endpoint(
    delete,
    path = "/api/todos/{id}",
    responses(
        (status = 200, description = "Todo deleted successfully"),
        (status = 401, description = "Unauthorized to delete Todo"),
        (status = 404, description = "Todo not found", body = TodoError, example = json!(TodoError::NotFound(String::from("id = 1"))))
    ),
    params(
        ("id" = i32, Path, description = "Id of todo item to delete")
    ),
    security(
        ("api_key" = [])
    )
)]
#[handler]
pub async fn delete_todo(req: &mut Request, res: &mut Response) {
    let id = req.param::<u64>("id").unwrap();
    tracing::debug!(id = ?id, "delete todo");

    let mut vec = STORE.lock().await;

    let len = vec.len();
    vec.retain(|todo| todo.id != id);

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
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let resp = TestClient::post("http://127.0.0.1:5800/api/todos")
            .json(&test_todo())
            .send(super::route())
            .await;

        assert_eq!(resp.status_code().unwrap(), StatusCode::CREATED);
        let resp = TestClient::post("http://127.0.0.1:5800/api/todos")
            .json(&test_todo())
            .send(super::route())
            .await;

        assert_eq!(resp.status_code().unwrap(), StatusCode::BAD_REQUEST);
    }

    fn test_todo() -> Todo {
        Todo {
            id: 1,
            text: "test todo".into(),
            completed: false,
        }
    }
}
