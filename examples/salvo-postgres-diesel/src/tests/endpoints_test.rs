
#[cfg(test)]
pub mod tests{

    use salvo::prelude::*;
    use salvo::test::TestClient;
    use std::sync::Arc;

    use crate::database::db::establish_connection_pool;
    use crate::routes::users::get_users_router;

    #[tokio::test]
    async fn test_hello_world() {

        let router = Router::new().get(crate::hello_world);

        // let service = Service::new(router);
        // let client = TestClient::new(service);

        let response = TestClient::get("http://localhost/").send(router).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));

        // let body = response.body.to_string();
        // assert_eq!(body, "Hello world");
    }

    #[tokio::test]
    async fn test_hello_with_query() {
        let pool = Arc::new(establish_connection_pool());
        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(Router::with_path("hello").get(crate::hello));

        // let service = Service::new(router);
        // let client = TestClient::new(service);

        let response = TestClient::get("http://localhost/hello?name=Darix").send(router).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));

        // let body = response.body;
        // assert_eq!(body, "Hello, Darix!");
    }

    #[tokio::test]
    async fn test_users_login_route_failure() {
        // Initialize DB pool
        let pool = Arc::new(establish_connection_pool());

        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(get_users_router());

        // let service = Service::new(router);
        // let client = TestClient::new(service);

        // Example POST request to login
        let response = TestClient::post("http://localhost/users/login")
            .json(&serde_json::json!({
                "username": "testuser",
                "password": "testpassword"
            }))
            .send(router)
            .await;

        // Depending on your auth logic, status might be OK or UNAUTHORIZED
        // println!("status code {:?}", response.status_code);
        // println!("Response in login : {:?}", response);
        assert!(response.status_code == Some(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn test_users_login_route_success() {
        // Initialize DB pool
        let pool = Arc::new(establish_connection_pool());

        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(get_users_router());

        // let service = Service::new(router);
        // let client = TestClient::new(service);

        // Example POST request to login
        let response = TestClient::post("http://localhost/users/login")
            .json(&serde_json::json!({
                "username": "samanidarix@gmail.com",
                "password": "Testpassword15$"
            }))
            .send(router)
            .await;

        // Depending on your auth logic, status might be OK or UNAUTHORIZED
        // println!("status code {:?}", response.status_code);
        // println!("Response in login : {:?}", response);
        assert!(response.status_code == Some(StatusCode::OK));
    }

    #[tokio::test]
    async fn test_protected_users_me_failure() {
        let pool = Arc::new(establish_connection_pool());

        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(get_users_router());

        // let service = Service::new(router);
        //let client = TestClient::new(service);

        // Attempt GET /users/me without auth header
        let response = TestClient::get("http://localhost/users/me")
                        .add_header("authentification", "authenfictaion", false)
                        .send(router).await;
        // println!("Response in test me: {:?}", response);
        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    }

    #[tokio::test]
    async fn test_protected_users_me_success() {
        let pool = Arc::new(establish_connection_pool());

        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(get_users_router());

        // let service = Service::new(router);
        //let client = TestClient::new(service);

        // Attempt GET /users/me without auth header
        let response = TestClient::get("http://localhost/users/me")
                        .add_header("authentification", "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJ1c2VybmFtZSI6InNhbWFuaWRhcml4QGdtYWlsLmNvbSIsImV4cCI6MTc2MzM3NjQzNH0.HmV5nmIDnqd10YtyIRJx737-nyiJLmzi7udHwnAwyyE", false)
                        .send(router).await;
        println!("Response in test me: {:?}", response);
        assert_eq!(response.status_code, Some(StatusCode::OK));
    }

    #[tokio::test]
    async fn test_users_update_success() {
        // Initialize DB pool
        let pool = Arc::new(establish_connection_pool());

        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(get_users_router());

        // let service = Service::new(router);
        // let client = TestClient::new(service);

        // Example POST request to login
        let response = TestClient::put(format!("http://localhost/users/{}", "73195821-51e4-407d-86b5-deadef4d43a5"))
            .add_header("authentification", "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJ1c2VybmFtZSI6InNhbWFuaWRhcml4QGdtYWlsLmNvbSIsImV4cCI6MTc2MzM3NjQzNH0.HmV5nmIDnqd10YtyIRJx737-nyiJLmzi7udHwnAwyyE", true)
            .json(&serde_json::json!({
                "fullname": "Darix SAMANI SIEWE"
            }))
            .send(router)
            .await;

        // Depending on your auth logic, status might be OK or UNAUTHORIZED
        // println!("status code {:?}", response.status_code);
        println!("Response in login : {:?}", response);
        assert!(response.status_code == Some(StatusCode::OK));
    }


    #[tokio::test]
    async fn test_users_update_failure() {
        // Initialize DB pool
        let pool = Arc::new(establish_connection_pool());

        let router = Router::new()
            .hoop(affix_state::inject(pool))
            .push(get_users_router());

        // let service = Service::new(router);
        // let client = TestClient::new(service);

        // Example POST request to login
        let response = TestClient::put(format!("http://localhost/users/{}", "73195821-51e4-407d-86b5-deadef4d43a5"))
            .add_header("authentification", "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJ1c2VybmFtZSI6InNhbWFuaWRhcml4QGdtYWlsLmNvbSIsImV4cCI6MTc2MzM3NjQzNH0.HmV5nmIDnqd10YtyIRJx737-nyiJLmzi7udHwnAwyyE", true)
            .json(&serde_json::json!({
                "username": "samanidarix@gmail.com",
                "full_name": "Darix SAMANI SIEWE"
            }))
            .send(router)
            .await;

        // Depending on your auth logic, status might be OK or UNAUTHORIZED
        // println!("status code {:?}", response.status_code);
        // println!("Response in login : {:?}", response);
        assert!(response.status_code == Some(StatusCode::BAD_REQUEST));
    }
}