use std::sync::Arc;
use migration::{Migrator, MigratorTrait};
use salvo::cors::{AllowOrigin, Cors};
use salvo::{Router, Service, affix_state};
use salvo::http::Method;
use crate::database::db::establish_connection_pool;
use crate::{hello, hello_world};
use crate::routes::users::get_users_router;

pub async fn route() -> Router {  
    
        let connection = Arc::new(establish_connection_pool().await);

        let db = &*connection ;

    // automigration schemas of database
    let _result = db.get_schema_registry("salvo-postgres-seaorm::models::*").sync(db).await;

    if _result.is_err(){
         // Run Migration on startup when the are error in automigration
        let result = Migrator::up(db, None).await;
        
        if result.is_err(){
            eprintln!("Error during the migration")
        }
    }

        let cors = Cors::new()
            .allow_origin(AllowOrigin::any())
            .allow_methods(vec![Method::GET, Method::POST, Method::DELETE, Method::PUT])
            .allow_headers("authorization")
            .allow_headers("authentication")
            .into_handler();


        let router = Router::new()
            .hoop(cors)
            .hoop(affix_state::inject(connection))
            .get(hello_world)
            .push(Router::with_path("hello").get(hello))
            .push(get_users_router())
            .push(get_users_router());

        router 
    }

pub fn service(router: Router) -> Service{
    Service::new(router)
}

#[cfg(test)]
pub mod tests{
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};
    use crate::schemas::users::{UserResponseModel, UserSuccessResponseModel};
    use crate::schemas::{ErrorResponseModel, TokenResponseModel};
    

    #[tokio::test]
    async fn test_01_test_hello_world() {

        let service = super::service(super::route().await);

        let mut response = TestClient::get("http://localhost/")
                        .send(&service)
                        .await;
        assert_eq!(response.status_code.clone(), Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Hello world")

    }

    #[tokio::test]
    async fn test_02_test_hello_with_query() {
        
        let service = super::service(super::route().await);
        let mut response = TestClient::get("http://localhost/hello?name=Darix")
                .send(&service)
                .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Hello, Darix!")

    }

    #[tokio::test]
    async fn test_03_test_users_login_route_failure() {
        
        let service = super::service(super::route().await);

        let mut response = TestClient::post("http://localhost/users/login")
            .json(&serde_json::json!({
                "username": "testuser",
                "password": "testpassword"
            }))
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
        

        let error: ErrorResponseModel = response
            .take_json()
            .await
            .expect("Failed to parse JSON response");

        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({ "detail": "🚫 Invalid username or password" })
        );

    }

    #[tokio::test]
    async fn test_04_test_create_users_route_failure() {
        let service = super::service(super::route().await);

        let response=TestClient::post("http://localhost/users")
            .json(&serde_json::json!({
                "email": "samanidarix@gmail.com",
                // "fullname": "Darix SAMANI SIEWE",
                "password": "Testpassword15$"
            }))
            .send(&service)
            .await;

        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn test_05_test_create_users_route_success() {
        let service = super::service(super::route().await);

        let mut response=TestClient::post("http://localhost/users")
            .json(&serde_json::json!({
                "email": "samanidarix@gmail.com",
                "fullname": "Darix SAMANI SIEWE",
                "password": "Testpassword15$"
            }))
            .send(&service)
            .await;

        assert_eq!(response.status_code, Some(StatusCode::CREATED));
        assert_eq!(response.take_string().await.unwrap(), "✅ User 'samanidarix@gmail.com' created successfully!");
    }

    #[tokio::test]
    async fn test_06_test_create_users_route_failure_user_already_exist() {
        let service = super::service(super::route().await);

        let mut response=TestClient::post("http://localhost/users")
            .json(&serde_json::json!({
                "email": "samanidarix@gmail.com",
                "fullname": "Darix SAMANI SIEWE",
                "password": "Testpassword15$"
            }))
            .send(&service)
            .await;

        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
        
        let response: ErrorResponseModel = response.take_json()
                                    .await
                                    .expect("Failed to parse JSON");
        assert_eq!(
            serde_json::to_value(&response).unwrap(), 
            serde_json::json!({"detail":"🚫 User 'samanidarix@gmail.com' already exists"}))
    }


    #[tokio::test]
    async fn test_07_test_users_login_route_failure_2() {
        let service = super::service(super::route().await);

        let mut response = TestClient::post("http://localhost/users/login")
            .json(&serde_json::json!({
                "username": "samanidarix@gmail.com",
                "password": "wrongpassword"
            }))
            .send(&service)
            .await;
        assert!(response.status_code == Some(StatusCode::BAD_REQUEST));

        let error: ErrorResponseModel = response
            .take_json()
            .await
            .expect("Failed to parse JSON response");

        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({ "detail": "🚫 Invalid username or password" })
        );

    }

    #[tokio::test]
    async fn test_08_test_users_login_route_success() {
        let service = super::service(super::route().await);

        let mut response = TestClient::post("http://localhost/users/login")
            .json(&serde_json::json!({
                "username": "samanidarix@gmail.com",
                "password": "Testpassword15$"
            }))
            .send(&service)
            .await;
        eprintln!("{:?}", response.body);
        assert_eq!(response.status_code, Some(StatusCode::OK));

        let response: TokenResponseModel = response
                            .take_json()
                            .await
                            .expect("Failed to parse json");
        assert_eq!(response.token_type, "Bearer");
        
    }

    #[tokio::test]
    async fn test_09_test_protected_users_me_failure() {
        let service = super::service(super::route().await);

        
        let mut response = TestClient::get("http://localhost/users/me")
                        .add_header("authentication", "authentication", false)
                        .send(&service)
                        .await;

        let error: ErrorResponseModel = response
                                .take_json()
                                .await
                                .expect("Failed to parse JSON");

        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({"detail": "Invalid or malformed token"})
        )
    }

    #[tokio::test]
    async fn test_10_test_protected_users_me_failure() {
        let service = super::service(super::route().await);

        
        let mut response = TestClient::get("http://localhost/users/me")
                        .add_header("authentication", "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJ1c2VybmFtZSI6InNhbWFuaWRhcml4QGdtYWlsLmNvbSIsImV4cCI6MTc2MzM3NjQzNH0.HmV5nmIDnqd10YtyIRJx737-nyiJLmzi7udHwnAwyyE", false)
                        .send(&service)
                        .await;

        let error: ErrorResponseModel = response
                                .take_json()
                                .await
                                .expect("Failed to parse JSON");

        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({"detail": "Invalid or malformed token"})
        )
    }

    #[tokio::test]
    async fn test_11_test_protected_users_me_success() {
        
        let service = super::service(super::route().await);

        let mut token_response = TestClient::post("http://localhost/users/login")
                                        .json(&serde_json::json!({
                                                "username": "samanidarix@gmail.com",
                                                "password": "Testpassword15$"
                                        }))
                                        .send(&service)
                                        .await;

        
        let token: TokenResponseModel = token_response
                                                .take_json()
                                                .await
                                                .expect("Failed to parse JSON");

        let mut response = TestClient::get("http://localhost/users/me")
                        .add_header("authentication", token.token, false)
                        .send(&service)
                        .await;
        let user_information: UserResponseModel = response
                                        .take_json()
                                        .await
                                        .expect("Failed to parse JSON");
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(user_information.email, "samanidarix@gmail.com");
        assert_eq!(user_information.full_name, "Darix SAMANI SIEWE");

    }

    #[tokio::test]
    async fn test_12_test_users_update_success() {
        let service = super::service(super::route().await);

        let mut token_response = TestClient::post("http://localhost/users/login")
                                        .json(&serde_json::json!({
                                                "username": "samanidarix@gmail.com",
                                                "password": "Testpassword15$"
                                        }))
                                        .send(&service)
                                        .await;

        
        let token: TokenResponseModel = token_response
                                                .take_json()
                                                .await
                                                .expect("Failed to parse JSON");

        let mut response = TestClient::get("http://localhost/users/me")
                        .add_header("authentication", token.token.clone(), false)
                        .send(&service)
                        .await;
        let user_information: UserResponseModel = response
                                        .take_json()
                                        .await
                                        .expect("Failed to parse JSON");

        let mut response = TestClient::put(format!("http://localhost/users/{}", user_information.id))
            .add_header("authentication", token.token, true)
            .json(&serde_json::json!({
                "fullname": "Darix SAMANI"
            }))
            .send(&service)
            .await;

        let user_update: UserSuccessResponseModel = response
                                    .take_json()
                                    .await
                                    .expect("Failed to parse JSON");
        println!("debug: {:?}", user_update);

        assert_eq!(response.status_code, Some(StatusCode::OK));
        //assert_eq!(user_update.email, "samanidarix@gmail.com");
        assert_eq!(user_update.full_name, "Darix SAMANI");
    }


    #[tokio::test]
    async fn test_13_test_users_update_failure() {
        let service = super::service(super::route().await);

        let mut token_response = TestClient::post("http://localhost/users/login")
                                        .json(&serde_json::json!({
                                                "username": "samanidarix@gmail.com",
                                                "password": "Testpassword15$"
                                        }))
                                        .send(&service)
                                        .await;

        
        let token: TokenResponseModel = token_response
                                                .take_json()
                                                .await
                                                .expect("Failed to parse JSON");

        let mut response = TestClient::get("http://localhost/users/me")
                        .add_header("authentication", token.token.clone(), false)
                        .send(&service)
                        .await;
        let user_information: UserResponseModel = response
                                        .take_json()
                                        .await
                                        .expect("Failed to parse JSON");

        let response = TestClient::put(format!("http://localhost/users/{}", user_information.id))
            .add_header("authentication", token.token, true)
            .json(&serde_json::json!({
                "username": "samanidarix@gmail.com",
                "full_name": "Darix SAMANI SIEWE"
            }))
            .send(&service)
            .await;

        assert!(response.status_code == Some(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn test_14_test_users_delete_success() {
        let service = super::service(super::route().await);

        let mut token_response = TestClient::post("http://localhost/users/login")
                                        .json(&serde_json::json!({
                                                "username": "samanidarix@gmail.com",
                                                "password": "Testpassword15$"
                                        }))
                                        .send(&service)
                                        .await;

        
        let token: TokenResponseModel = token_response
                                                .take_json()
                                                .await
                                                .expect("Failed to parse JSON");

        let mut response = TestClient::get("http://localhost/users/me")
                        .add_header("authentication", token.token.clone(), false)
                        .send(&service)
                        .await;
        let user_information: UserResponseModel = response
                                        .take_json()
                                        .await
                                        .expect("Failed to parse JSON");

        let mut response = TestClient::delete(format!("http://localhost/users/{}", user_information.id))
            .add_header("authentication", token.token, true)
            .send(&service)
            .await;

        let user_delete: UserSuccessResponseModel = response
                                    .take_json()
                                    .await
                                    .expect("Failed to parse JSON");

        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(user_delete.email, "samanidarix@gmail.com");
        assert_eq!(user_delete.full_name, "Darix SAMANI");
    }
}