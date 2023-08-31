#[macro_use]
extern crate rbatis;
extern crate rbdc;

use once_cell::sync::Lazy;
use rbatis::RBatis;
use rbdc_mysql::driver::MysqlDriver;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

pub static RB: Lazy<RBatis> = Lazy::new(RBatis::new);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
}

// crud!(User{});
impl_select!(User{select_by_id(id:String) -> Option => "`where id = #{id} limit 1`"});
#[handler]
pub async fn get_user(req: &mut Request, res: &mut Response) {
    let uid = req.query::<i64>("uid").unwrap();
    let data = User::select_by_id(&mut RB.clone(), uid.to_string()).await.unwrap();
    println!("{data:?}");
    res.render(serde_json::to_string(&data).unwrap());
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // mysql connect info
    let mysql_uri = "mysql://root:123456@localhost/test";
    RB.init(MysqlDriver {}, mysql_uri).unwrap();

    // router
    let router = Router::with_path("users").get(get_user);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
