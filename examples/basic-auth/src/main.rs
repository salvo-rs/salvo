use salvo::extra::authorization::{
    AuthorizationHandler, AuthorizationResult, AuthorizationType, AuthorizationValidator,
};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(route()).await;
}
fn route() -> Router {
    let auth_handler = AuthorizationHandler::new(AuthorizationType::Basic, Validator);
    Router::with_hoop(auth_handler).handle(hello)
}
#[fn_handler]
async fn hello() -> &'static str {
    "Hello"
}

struct Validator;
#[async_trait]
impl AuthorizationValidator for Validator {
    async fn validate(&self, data: AuthorizationResult) -> bool {
        if let AuthorizationResult::Basic(u) = data {
            return u.0 == "root" && u.1 == "pwd";
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_basic_auth() {
        let service = Service::new(super::route());

        let content = TestClient::get("http://127.0.0.1:7878/")
            .basic_auth("root", Some("pwd"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Hello"));

        let content = TestClient::get("http://127.0.0.1:7878/")
            .basic_auth("root", Some("pwd2"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));
    }
}
