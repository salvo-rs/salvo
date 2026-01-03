use crate::database::db::establish_connection_pool;
use crate::routes::posts::get_posts_router;
use crate::routes::users::get_users_router;
use salvo::cors::AllowOrigin;
use salvo::http::Method;
use salvo::prelude::*;
use salvo_oapi::OpenApi;
use salvo_oapi::endpoint;
use salvo_oapi::extract::QueryParam;
use std::sync::Arc;
use crate::database::db::DbPool;
use salvo::cors::Cors;
use migration::{Migrator, MigratorTrait};

pub mod database;
pub mod models;
pub mod routes;
pub mod schemas;
pub mod auth;
pub mod utils;
pub mod tests;

#[endpoint(
    tags("Main"),
    summary = "hello",
    description = "description  of the  main endpoint"
)]
pub async fn hello(name: QueryParam<String, false>, res: &mut Response, depot: &mut Depot) {
    println!("{:?}", name);
    let _pool = depot.obtain::<Arc<DbPool>>().unwrap();
    // let mut _conn = pool.get().expect("Failed to get DB connection");
    res.status_code(StatusCode::OK);
    res.render(format!("Hello, {}!", name.clone().unwrap()))
}

#[endpoint(
    tags("Hello"),
    summary = "Just Print hello world",
    description = "description of the handle/endpoint to print hello world"
)]
pub async fn hello_world(res: &mut Response) -> Result<&'static str, salvo::Error> {
    res.status_code(StatusCode::OK);
    Ok("Hello world")
}

#[tokio::main]
async fn main() {
    

    // Setup tracing for print debug

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .init();

    // Establish connection poll with smart pointer Arc

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

   
    // Setup cors origin
    let cors = Cors::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods(vec![Method::GET, Method::POST, Method::DELETE, Method::PUT, Method::PATCH])
        .allow_headers("authorization")
        .allow_headers("authentication")
        .into_handler();


    // Setup router

    let router = Router::new()
        .hoop(affix_state::inject(connection))
        .get(hello_world)
        .push(Router::with_path("hello").get(hello))
        .push(get_users_router())
        .push(get_posts_router());

    // Setup OPENAPI docs
    
    let doc = OpenApi::new("Salvo Postgresql Boilerplate", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("/docs"));

     let service = Service::new(router).hoop(cors);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;

    Server::new(acceptor).serve(service).await;
}
