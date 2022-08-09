#[macro_use]
extern crate rbatis;
extern crate rbdc;

use rbatis::Rbatis;
use salvo::prelude::*;
use serde::Serialize;
use rbdc_mysql::driver::MysqlDriver;

static RB: Rbatis = Rbatis::new();

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
    let data = User::select_by_id(&mut RB, uid.to_string()).await;
    res.render(serde_json::to_string(&data).unwrap());
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // mysql connect info
    let mysql_uri = "postgres://postgres:password@localhost/test";
    RB.link(MysqlDriver {}, mysql_uri).await.unwrap();

    // router
    let router = Router::with_path("users").get(get_user);

    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
