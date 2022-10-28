# 使用數據庫

### Diesel

```rust
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PoolError, PooledConnection};
use once_cell::sync::OnceCell;
use salvo::prelude::*;

const DB_URL: &str = "postgres://benchmarkdbuser:benchmarkdbpass@tfb-database/hello_world";
type PgPool = Pool<ConnectionManager<PgConnection>>;

static DB_POOL: OnceCell<PgPool> = OnceCell::new();

fn connect() -> Result<PooledConnection<ConnectionManager<PgConnection>>, PoolError> {
    DB_POOL.get().unwrap().get()
}
fn build_pool(database_url: &str, size: u32) -> Result<PgPool, PoolError> {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    diesel::r2d2::Pool::builder()
        .max_size(size)
        .min_idle(Some(size))
        .test_on_check_out(false)
        .idle_timeout(None)
        .max_lifetime(None)
        .build(manager)
}

fn main() {
    DB_POOL
        .set(build_pool(&DB_URL, 10).expect(&format!("Error connecting to {}", &DB_URL)))
        .ok();
}

#[handler]
async fn show_article(req: &mut Request, res: &mut Response) -> Result<(), Error> {
    let id: i64 = req.param::<i64>("id").unwrap_or_default();
    let conn = connect()?;
    let article = articles::table.find(id).first::<Article>(&conn)?;
    res.render(Json(row));
    Ok(())
}
```

### Sqlx

```rust
use sqlx::{Pool, PgPool};
use once_cell::sync::OnceCell;

pub static DB_POOL: OnceCell<PgPool> = OnceCell::new();

pub fn db_pool() -> &PgPool {
    DB_POOL.get().unwrap()
}

pub async fn make_db_pool(db_url: &str) -> PgPool {
    Pool::connect(&db_url).await.unwrap()
}

#[tokio::main]
async fn main() {
    let pool = make_db_pool().await;
    DB_POOL.set(pool).unwrap();
}
```

### rbatis

```toml
[dependencies]
async-std = "1.11.0"
fast_log = "1.5.24"
log = "0.4.17"
once_cell = "1.12.0"
rbatis = "4.0.7"
rbdc = "0.1.2"
rbdc-mysql = "0.1.7"
rbs = "0.1.2"
salvo = { path = "../../salvo" }
serde = { version = "1.0.143", features = ["derive"] }
tokio = { version = "1.20.1", features = ["macros"] }
tracing = "0.1.36"
tracing-subscriber = "0.3.15"
serde_json = "1.0"
```

```rust
#[macro_use]
extern crate rbatis;
extern crate rbdc;

use once_cell::sync::Lazy;
use rbatis::Rbatis;
use salvo::prelude::*;
use serde::{Serialize, Deserialize};
use rbdc_mysql::driver::MysqlDriver;

pub static RB: Lazy<Rbatis> = Lazy::new(|| Rbatis::new());

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
}

impl_select!(User{select_by_id(id:String) -> Option => "`where id = #{id} limit 1`"});
#[handler]
pub async fn get_user(req: &mut Request, res: &mut Response) {
    let uid = req.query::<i64>("uid").unwrap();
    let data = User::select_by_id(&mut RB.clone(), uid.to_string()).await.unwrap();
    println!("{:?}", data);
    res.render(serde_json::to_string(&data).unwrap());
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // mysql connect info
    let mysql_uri = "mysql://root:123456@localhost/test";
    RB.link(MysqlDriver {}, mysql_uri).await.unwrap();

    // router
    let router = Router::with_path("users").get(get_user);

    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

```