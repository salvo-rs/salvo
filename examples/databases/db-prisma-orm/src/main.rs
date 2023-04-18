use async_trait::async_trait;
use salvo::prelude::TcpListener;
use salvo::writer::Json;
use salvo::{handler, Depot, Error, FlowCtrl, Handler, Listener, Request, Response, Result, Router, Server};

use std::collections::HashMap;
use std::sync::Arc;

pub mod db;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let prisma_client = Arc::new(db::new_client().await.unwrap());

    #[cfg(debug)]
    prisma_client._db_push(false).await.unwrap();

    let router = Router::with_hoop(SetDB(prisma_client)).get(get).post(post);
    let addr = "127.0.0.1:5800";

    // Server::new(TcpListener::bind(addr)).serve(router).await;
    let acceptor = TcpListener::new(addr).bind().await;
    Server::new(acceptor).serve(router).await;
}

struct SetDB(Arc<db::PrismaClient>);

#[async_trait]
impl Handler for SetDB {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response, _ctrl: &mut FlowCtrl) {
        _depot.inject(self.0.clone());
        _ctrl.call_next(_req, _depot, _res).await;
    }
}

type Database = std::sync::Arc<db::PrismaClient>;

#[handler]
async fn get(depot: &mut Depot, res: &mut Response) -> Result<()> {
    let db = depot.obtain::<Database>().unwrap();
    let users = db
        .user()
        .find_many(vec![])
        .exec()
        .await
        .map_err(|e| Error::Other(e.to_string().into()))?;
    res.render(Json(users));
    Ok(())
}

#[handler]
async fn post(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<()> {
    let db = depot.obtain::<Database>().unwrap();
    let user = req.parse_body::<HashMap<String, String>>().await.map_err(|e| {
        tracing::error!("{}", e);
        e
    })?;
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
    res.render("ok");
    Ok(())
}
