use async_trait::async_trait;
use salvo::prelude::TcpListener;
use salvo::writing::Json;
use salvo::{handler, Depot, Error, FlowCtrl, Handler, Listener, Request, Response, Result, Router, Server};

use std::collections::HashMap;
use std::sync::Arc;

pub mod db;

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();
    // Create and wrap Prisma client in Arc for thread-safe sharing
    let prisma_client = Arc::new(db::new_client().await.unwrap());

    // In debug mode, push database schema changes
    #[cfg(debug)]
    prisma_client._db_push(false).await.unwrap();

    // Configure router with database middleware and handlers
    let router = Router::with_hoop(SetDB(prisma_client)).get(get).post(post);
    let addr = "0.0.0.0:5800";

    // Start server on port 5800
    let acceptor = TcpListener::new(addr).bind().await;
    Server::new(acceptor).serve(router).await;
}

// Middleware to inject Prisma client into request context
struct SetDB(Arc<db::PrismaClient>);

#[async_trait]
impl Handler for SetDB {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response, _ctrl: &mut FlowCtrl) {
        // Store Prisma client in request depot
        _depot.inject(self.0.clone());
        // Continue request processing
        _ctrl.call_next(_req, _depot, _res).await;
    }
}

// Type alias for thread-safe Prisma client reference
type Database = std::sync::Arc<db::PrismaClient>;

// Handler for retrieving all users
#[handler]
async fn get(depot: &mut Depot, res: &mut Response) -> Result<()> {
    // Get database client from depot
    let db = depot.obtain::<Database>().unwrap();
    // Query all users from database
    let users = db
        .user()
        .find_many(vec![])
        .exec()
        .await
        .map_err(|e| Error::Other(e.to_string().into()))?;
    // Return users as JSON response
    res.render(Json(users));
    Ok(())
}

// Handler for creating a new user
#[handler]
async fn post(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    // Get database client from depot
    let db = depot.obtain::<Database>().unwrap();
    // Parse user data from request body
    let user = req.parse_body::<HashMap<String, String>>().await.map_err(|e| {
        tracing::error!("{}", e);
        e
    })?;
    
    // Create new user in database
    db.user()
        .create(
            user.get("username").unwrap().to_string(),
            user.get("email").unwrap().to_string(),
            vec![],
        )
        .exec()
        .await
        .map_err(|e| {
            tracing::error!("{}", e);
            Error::Other(e.to_string().into())
        })?;
    
    // Return success response
    res.render("ok");
    Ok(())
}
