use salvo::prelude::*;

use juniper::http::GraphQLRequest;
use schema::create_schema;

use crate::schema::DatabaseContext;

pub mod mutation;
pub mod query;
pub mod schema;

#[tokio::main]
async fn main() {
    let router = Router::new().push(Router::with_path("graphql").post(graphql));
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[handler]
async fn graphql(req: &mut Request, res: &mut Response) {
    let schema = create_schema();
    let context = DatabaseContext::new();
    let data = req.parse_json::<GraphQLRequest>().await.unwrap();
    let response = data.execute(&schema, &context).await;
    res.render(Json(response))
}
