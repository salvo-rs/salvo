use salvo::prelude::*;

use juniper::http::GraphQLRequest;
use schema::create_schema;

use crate::schema::DatabaseContext;

pub mod mutation;
pub mod query;
pub mod schema;

#[tokio::main]
async fn main() {
    // Create router with GraphQL endpoint
    let router = Router::new().push(Router::with_path("graphql").post(graphql));
    // Bind server to port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    // Start the server
    Server::new(acceptor).serve(router).await;
}

// Handler for GraphQL requests
#[handler]
async fn graphql(req: &mut Request, res: &mut Response) {
    // Create GraphQL schema
    let schema = create_schema();
    // Initialize database context
    let context = DatabaseContext::new();
    // Parse incoming GraphQL request
    let data = req.parse_json::<GraphQLRequest>().await.unwrap();
    // Execute GraphQL query
    let response = data.execute(&schema, &context).await;
    // Return JSON response
    res.render(Json(response))
}
